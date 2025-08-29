//! Main text correction engine
//!
//! This module provides the primary implementation of text correction
//! combining character-level fixes and dictionary-based corrections.

use super::character::UniversalCharacterCorrector;
use super::config::CorrectionConfig;
use super::traits::{CharacterCorrector, DictionaryCorrector, TextCorrector};
use crate::blocks::{Block, BlockType};
use crate::correction::dictionary::{SmartCorrectionConfig, SmartCorrector};
use anyhow::Result;
use once_cell::sync::OnceCell;
use tracing::{info, warn};

/// Global correction engine instance
static GLOBAL_CORRECTOR: OnceCell<Option<FerrulesCorrectionEngine>> = OnceCell::new();

/// Main text correction engine that combines all correction strategies
pub struct FerrulesCorrectionEngine {
    character_corrector: UniversalCharacterCorrector,
    dictionary_corrector: Option<SmartCorrector>,
    config: CorrectionConfig,
}

impl FerrulesCorrectionEngine {
    /// Create a new correction engine with the given configuration
    pub fn new(config: CorrectionConfig) -> Result<Self> {
        let character_corrector = UniversalCharacterCorrector::default();

        let dictionary_corrector = if config.enable_dictionary_corrections {
            let smart_config = SmartCorrectionConfig {
                enable_dictionary_corrections: true,
                confidence_threshold: config.confidence_threshold,
                cache_size: config.cache_size,
                cache_ttl_seconds: config.cache_ttl_seconds,
                fuzzy_match_threshold: config.fuzzy_match_threshold,
            };

            match SmartCorrector::new(smart_config) {
                Ok(corrector) => {
                    info!("✅ Dictionary corrector initialized");
                    Some(corrector)
                }
                Err(e) => {
                    warn!("⚠️ Failed to initialize dictionary corrector: {}", e);
                    None
                }
            }
        } else {
            info!("📝 Dictionary corrections disabled");
            None
        };

        info!(
            "🔧 Correction engine initialized (cache: {}, ttl: {}s, dict: {})",
            config.cache_size, config.cache_ttl_seconds, config.enable_dictionary_corrections
        );

        Ok(Self {
            character_corrector,
            dictionary_corrector,
            config,
        })
    }

    /// Initialize the global correction engine
    pub fn initialize_global(config: CorrectionConfig) -> Result<()> {
        let engine = match Self::new(config) {
            Ok(engine) => Some(engine),
            Err(e) => {
                warn!("Failed to initialize correction engine: {}", e);
                None
            }
        };

        GLOBAL_CORRECTOR
            .set(engine)
            .map_err(|_| anyhow::anyhow!("Global correction engine already initialized"))?;

        Ok(())
    }
}

impl CharacterCorrector for FerrulesCorrectionEngine {
    fn correct_characters(&self, text: &str) -> String {
        self.character_corrector.correct_characters(text)
    }
}

impl DictionaryCorrector for FerrulesCorrectionEngine {
    fn correct_words(&self, text: &str) -> String {
        if let Some(ref corrector) = self.dictionary_corrector {
            // Use tokio runtime to handle async correction
            match tokio::runtime::Handle::try_current() {
                Ok(handle) => {
                    tokio::task::block_in_place(|| handle.block_on(corrector.correct_text(text)))
                }
                Err(_) => {
                    // Create new runtime if none exists
                    if let Ok(rt) = tokio::runtime::Runtime::new() {
                        rt.block_on(corrector.correct_text(text))
                    } else {
                        text.to_string()
                    }
                }
            }
        } else {
            text.to_string()
        }
    }

    fn is_available(&self) -> bool {
        self.dictionary_corrector.is_some()
    }
}

impl TextCorrector for FerrulesCorrectionEngine {
    fn correct_text(&self, text: &str) -> String {
        if text.is_empty() {
            return text.to_string();
        }

        // Apply character corrections first
        let character_corrected = self.correct_characters(text);

        // Then apply dictionary corrections if available and enabled
        if self.config.enable_dictionary_corrections {
            let final_corrected = self.correct_words(&character_corrected);

            if final_corrected != text {}

            final_corrected
        } else {
            character_corrected
        }
    }

    fn correct_block(&self, block: &mut Block) {
        match &mut block.kind {
            BlockType::TextBlock(text) => {
                if !text.text.is_empty() {
                    let corrected = self.correct_text(&text.text);
                    if corrected != text.text {
                        text.text = corrected;
                    }
                }
            }
            BlockType::Header(header) => {
                if !header.text.is_empty() {
                    let corrected = self.correct_text(&header.text);
                    if corrected != header.text {
                        header.text = corrected;
                    }
                }
            }
            BlockType::Footer(footer) => {
                if !footer.text.is_empty() {
                    let corrected = self.correct_text(&footer.text);
                    if corrected != footer.text {
                        footer.text = corrected;
                    }
                }
            }
            BlockType::Title(title) => {
                if !title.text.is_empty() {
                    let corrected = self.correct_text(&title.text);
                    if corrected != title.text {
                        title.text = corrected;
                    }
                }
            }
            BlockType::ListBlock(list) => {
                for item in &mut list.items {
                    if !item.is_empty() {
                        let corrected = self.correct_text(item);
                        if corrected != *item {
                            *item = corrected;
                        }
                    }
                }
            }
            _ => {
                // No text content to correct (Image, Table)
            }
        }
    }
}

/// Get the global correction engine instance
pub fn get_global_corrector() -> Option<&'static dyn TextCorrector> {
    GLOBAL_CORRECTOR
        .get()
        .and_then(|opt| opt.as_ref())
        .map(|engine| engine as &dyn TextCorrector)
}

/// Initialize global correction engine from environment
pub fn initialize_from_environment() -> Result<()> {
    let config = CorrectionConfig::from_environment();
    FerrulesCorrectionEngine::initialize_global(config)
}

/// Legacy compatibility function for existing code
pub fn get_global_correction_engine() -> Option<&'static FerrulesCorrectionEngine> {
    GLOBAL_CORRECTOR.get().and_then(|opt| opt.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::{Block, BlockType, TextBlock};

    fn create_test_engine() -> FerrulesCorrectionEngine {
        let config = CorrectionConfig {
            cache_size: 100,
            cache_ttl_seconds: 60,
            confidence_threshold: 0.7,
            enable_dictionary_corrections: true,
            fuzzy_match_threshold: 50,
        };
        FerrulesCorrectionEngine::new(config).unwrap()
    }

    #[tokio::test]
    async fn test_character_corrections() {
        let engine = create_test_engine();

        let result = engine.correct_characters("w)th");
        assert_eq!(result, "with");

        let result = engine.correct_characters("whic(");
        assert_eq!(result, "which");
    }

    #[test]
    fn test_text_correction() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let engine = create_test_engine();

            let result = engine.correct_text("w)th");
            // Should apply character corrections (and potentially dictionary corrections)
            assert!(result.contains("with") || result == "with");
        });
    }

    #[test]
    fn test_block_correction() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let engine = create_test_engine();

            let mut block = Block {
                id: 1,
                kind: BlockType::TextBlock(TextBlock {
                    text: "th)s )s a test".to_string(),
                }),
                pages_id: vec![],
                bbox: Default::default(),
            };

            engine.correct_block(&mut block);

            if let BlockType::TextBlock(text) = &block.kind {
                // Should have applied character corrections
                assert!(text.text != "th)s )s a test");
                assert!(text.text.contains("this is"));
            }
        });
    }
}
