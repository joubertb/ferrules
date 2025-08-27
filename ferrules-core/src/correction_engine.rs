use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Configuration structure for font-specific corrections
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontCorrectionConfig {
    pub description: String,
    pub corrections: HashMap<String, String>, // character -> corrected character
    pub confidence: f64,
    pub enabled: bool,
    pub is_subset: bool,
    pub has_tounicode: bool,
}

/// Pattern-based corrections for common corruption patterns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternCorrections {
    #[serde(flatten)]
    pub patterns: HashMap<String, String>, // corrupted pattern -> corrected pattern
}

/// Settings for the correction engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrectionSettings {
    pub enable_font_corrections: bool,
    pub enable_pattern_corrections: bool,
    pub confidence_threshold: f64,
    pub max_corrections_per_word: usize,
    pub enable_diagnostic_logging: bool,
}

impl Default for CorrectionSettings {
    fn default() -> Self {
        Self {
            enable_font_corrections: true,
            enable_pattern_corrections: true,
            confidence_threshold: 0.7,
            max_corrections_per_word: 3,
            enable_diagnostic_logging: false,
        }
    }
}

/// Main configuration file structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrectionConfigFile {
    pub version: String,
    pub description: String,
    pub last_updated: String,
    pub font_corrections: HashMap<String, FontCorrectionConfig>,
    pub pattern_corrections: HashMap<String, String>,
    pub settings: CorrectionSettings,
}

/// Statistics for correction application
#[derive(Debug, Default, Clone)]
pub struct CorrectionStats {
    pub font_corrections_applied: u64,
    pub pattern_corrections_applied: u64,
    pub total_characters_processed: u64,
    pub fonts_analyzed: HashMap<String, u64>,
}

/// Hot-reloadable correction engine
pub struct CorrectionEngine {
    config_path: PathBuf,
    experimental_config_path: Option<PathBuf>,
    config: Arc<RwLock<CorrectionConfigFile>>,
    experimental_config: Arc<RwLock<Option<CorrectionConfigFile>>>,
    stats: Arc<RwLock<CorrectionStats>>,
    last_reload: Arc<RwLock<SystemTime>>,
}

impl CorrectionEngine {
    /// Create a new correction engine with the specified config file
    pub fn new<P: AsRef<Path>>(config_path: P) -> Result<Self> {
        let config_path = config_path.as_ref().to_path_buf();
        let config = Self::load_config(&config_path)?;

        info!(
            "🔧 CorrectionEngine initialized with {} font corrections and {} pattern corrections",
            config.font_corrections.len(),
            config.pattern_corrections.len()
        );

        Ok(Self {
            config_path,
            experimental_config_path: None,
            config: Arc::new(RwLock::new(config)),
            experimental_config: Arc::new(RwLock::new(None)),
            stats: Arc::new(RwLock::new(CorrectionStats::default())),
            last_reload: Arc::new(RwLock::new(SystemTime::now())),
        })
    }

    /// Create with both main and experimental config files
    pub async fn with_experimental<P1: AsRef<Path>, P2: AsRef<Path>>(
        config_path: P1,
        experimental_path: P2,
    ) -> Result<Self> {
        let mut engine = Self::new(config_path)?;
        let experimental_path = experimental_path.as_ref().to_path_buf();

        if experimental_path.exists() {
            let experimental_config = Self::load_config(&experimental_path)?;
            *engine.experimental_config.write().await = Some(experimental_config);
            engine.experimental_config_path = Some(experimental_path);
            info!("🧪 Experimental corrections loaded");
        }

        Ok(engine)
    }

    /// Load configuration from file
    fn load_config<P: AsRef<Path>>(path: P) -> Result<CorrectionConfigFile> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| anyhow!("Failed to read config file: {}", e))?;

        let config: CorrectionConfigFile = serde_json::from_str(&content)
            .map_err(|e| anyhow!("Failed to parse config file: {}", e))?;

        debug!("📄 Loaded config version: {}", config.version);
        Ok(config)
    }

    /// Reload configuration from disk if file has been modified
    pub async fn reload_if_modified(&self) -> Result<bool> {
        let last_reload = *self.last_reload.read().await;

        // Check main config file modification time
        let metadata = std::fs::metadata(&self.config_path)?;
        let modified = metadata.modified()?;

        if modified > last_reload {
            info!("🔄 Configuration file modified, reloading...");
            let new_config = Self::load_config(&self.config_path)?;
            *self.config.write().await = new_config;
            *self.last_reload.write().await = SystemTime::now();

            info!("✅ Configuration reloaded successfully");
            return Ok(true);
        }

        // Check experimental config if present
        if let Some(ref exp_path) = self.experimental_config_path {
            if exp_path.exists() {
                let exp_metadata = std::fs::metadata(exp_path)?;
                let exp_modified = exp_metadata.modified()?;

                if exp_modified > last_reload {
                    info!("🔄 Experimental configuration file modified, reloading...");
                    let new_exp_config = Self::load_config(exp_path)?;
                    *self.experimental_config.write().await = Some(new_exp_config);
                    *self.last_reload.write().await = SystemTime::now();

                    info!("✅ Experimental configuration reloaded successfully");
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    /// Apply font-specific character corrections
    pub async fn apply_font_corrections(&self, text: &str, font_name: &str) -> Result<String> {
        let config = self.config.read().await;

        if !config.settings.enable_font_corrections {
            return Ok(text.to_string());
        }

        // Check main config
        if let Some(font_config) = config.font_corrections.get(font_name) {
            if font_config.enabled && font_config.confidence >= config.settings.confidence_threshold
            {
                return self
                    .apply_corrections(text, &font_config.corrections, font_name)
                    .await;
            }
        }

        // Check experimental config
        let exp_config = self.experimental_config.read().await;
        if let Some(ref exp_config) = *exp_config {
            if exp_config.settings.enable_font_corrections {
                if let Some(font_config) = exp_config.font_corrections.get(font_name) {
                    if font_config.enabled
                        && font_config.confidence >= exp_config.settings.confidence_threshold
                    {
                        return self
                            .apply_corrections(text, &font_config.corrections, font_name)
                            .await;
                    }
                }
            }
        }

        Ok(text.to_string())
    }

    /// Apply pattern-based corrections
    pub async fn apply_pattern_corrections(&self, text: &str) -> Result<String> {
        let config = self.config.read().await;

        if !config.settings.enable_pattern_corrections {
            return Ok(text.to_string());
        }

        let mut corrected = text.to_string();
        let mut corrections_applied = 0;

        // Apply main config patterns
        for (pattern, replacement) in &config.pattern_corrections {
            if corrections_applied >= config.settings.max_corrections_per_word {
                break;
            }

            if corrected.contains(pattern) {
                corrected = corrected.replace(pattern, replacement);
                corrections_applied += 1;

                if config.settings.enable_diagnostic_logging {
                    debug!(
                        "🔧 Pattern correction: '{}' -> '{}' in text",
                        pattern, replacement
                    );
                }
            }
        }

        // Apply experimental patterns if enabled
        let exp_config = self.experimental_config.read().await;
        if let Some(ref exp_config) = *exp_config {
            if exp_config.settings.enable_pattern_corrections {
                for (pattern, replacement) in &exp_config.pattern_corrections {
                    if corrections_applied >= config.settings.max_corrections_per_word {
                        break;
                    }

                    if corrected.contains(pattern) {
                        corrected = corrected.replace(pattern, replacement);
                        corrections_applied += 1;

                        if exp_config.settings.enable_diagnostic_logging {
                            debug!(
                                "🧪 Experimental pattern correction: '{}' -> '{}' in text",
                                pattern, replacement
                            );
                        }
                    }
                }
            }
        }

        // Update stats
        if corrections_applied > 0 {
            let mut stats = self.stats.write().await;
            stats.pattern_corrections_applied += corrections_applied as u64;
        }

        Ok(corrected)
    }

    /// Apply character-level corrections
    async fn apply_corrections(
        &self,
        text: &str,
        corrections: &HashMap<String, String>,
        font_name: &str,
    ) -> Result<String> {
        let mut corrected = String::new();
        let mut corrections_applied = 0;

        for ch in text.chars() {
            if let Some(replacement) = corrections.get(&ch.to_string()) {
                corrected.push_str(replacement);
                corrections_applied += 1;

                let config = self.config.read().await;
                if config.settings.enable_diagnostic_logging {
                    debug!(
                        "🔧 Font correction: '{}' -> '{}' (font: {})",
                        ch, replacement, font_name
                    );
                }
            } else {
                corrected.push(ch);
            }
        }

        // Update stats
        if corrections_applied > 0 {
            let mut stats = self.stats.write().await;
            stats.font_corrections_applied += corrections_applied as u64;
            *stats
                .fonts_analyzed
                .entry(font_name.to_string())
                .or_insert(0) += corrections_applied as u64;
        }

        let mut stats = self.stats.write().await;
        stats.total_characters_processed += text.len() as u64;

        Ok(corrected)
    }

    /// Get current correction statistics
    pub async fn get_stats(&self) -> CorrectionStats {
        self.stats.read().await.clone()
    }

    /// Reset statistics
    pub async fn reset_stats(&self) {
        let mut stats = self.stats.write().await;
        *stats = CorrectionStats::default();
    }

    /// Check if a font has known corrections
    pub async fn has_font_corrections(&self, font_name: &str) -> bool {
        let config = self.config.read().await;

        if config.font_corrections.contains_key(font_name) {
            return true;
        }

        let exp_config = self.experimental_config.read().await;
        if let Some(ref exp_config) = *exp_config {
            exp_config.font_corrections.contains_key(font_name)
        } else {
            false
        }
    }

    /// Get list of all configured fonts
    pub async fn get_configured_fonts(&self) -> Vec<String> {
        let config = self.config.read().await;
        let mut fonts: Vec<String> = config.font_corrections.keys().cloned().collect();

        let exp_config = self.experimental_config.read().await;
        if let Some(ref exp_config) = *exp_config {
            for font in exp_config.font_corrections.keys() {
                if !fonts.contains(font) {
                    fonts.push(font.clone());
                }
            }
        }

        fonts.sort();
        fonts
    }

    /// Log diagnostic information about a potentially problematic font
    pub async fn log_problematic_font(
        &self,
        font_name: &str,
        char_code: u16,
        extracted_char: char,
        context: &str,
    ) {
        let config = self.config.read().await;

        if config.settings.enable_diagnostic_logging {
            warn!("🚨 Potentially problematic font detected:");
            warn!("   Font: {}", font_name);
            warn!(
                "   Character: '{}' (U+{:04X}, code: {})",
                extracted_char, extracted_char as u32, char_code
            );
            warn!("   Context: '{}'", context);
            warn!(
                "   Has corrections: {}",
                self.has_font_corrections(font_name).await
            );
        }
    }
}
