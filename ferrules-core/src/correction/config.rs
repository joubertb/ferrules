//! Configuration management for text correction system

use super::traits::CorrectionConfiguration;
use std::env;

/// Configuration for the text correction system
///
/// This centralizes all configuration management and provides defaults
/// for when environment variables are not set.
#[derive(Debug, Clone)]
pub struct CorrectionConfig {
    /// Size of the correction cache (default: 10,000)
    pub cache_size: u64,

    /// Cache TTL in seconds (default: 3600 = 1 hour)
    pub cache_ttl_seconds: u64,

    /// Confidence threshold for corrections (default: 0.7)
    pub confidence_threshold: f64,

    /// Whether dictionary corrections are enabled (default: true)
    pub enable_dictionary_corrections: bool,

    /// Fuzzy match threshold for spell checking (default: 50)
    pub fuzzy_match_threshold: i64,
}

impl Default for CorrectionConfig {
    fn default() -> Self {
        Self::from_environment()
    }
}

impl CorrectionConfig {
    /// Create configuration from environment variables with fallback defaults
    pub fn from_environment() -> Self {
        let cache_size = env::var("FERRULES_CORRECTION_CACHE_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10_000);

        let cache_ttl_seconds = env::var("FERRULES_CORRECTION_CACHE_TTL_SECONDS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3600);

        let confidence_threshold = env::var("FERRULES_CORRECTION_CONFIDENCE_THRESHOLD")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.7);

        let enable_dictionary_corrections = env::var("FERRULES_ENABLE_DICTIONARY_CORRECTIONS")
            .map(|s| s.to_lowercase() != "false" && s != "0")
            .unwrap_or(true);

        let fuzzy_match_threshold = env::var("FERRULES_FUZZY_MATCH_THRESHOLD")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(50);

        Self {
            cache_size,
            cache_ttl_seconds,
            confidence_threshold,
            enable_dictionary_corrections,
            fuzzy_match_threshold,
        }
    }

    /// Create configuration with custom values
    pub fn new(
        cache_size: u64,
        cache_ttl_seconds: u64,
        confidence_threshold: f64,
        enable_dictionary_corrections: bool,
        fuzzy_match_threshold: i64,
    ) -> Self {
        Self {
            cache_size,
            cache_ttl_seconds,
            confidence_threshold,
            enable_dictionary_corrections,
            fuzzy_match_threshold,
        }
    }
}

impl CorrectionConfiguration for CorrectionConfig {
    fn cache_size(&self) -> u64 {
        self.cache_size
    }

    fn cache_ttl_seconds(&self) -> u64 {
        self.cache_ttl_seconds
    }

    fn confidence_threshold(&self) -> f64 {
        self.confidence_threshold
    }

    fn dictionary_corrections_enabled(&self) -> bool {
        self.enable_dictionary_corrections
    }
}

/// Environment variable documentation
///
/// Available configuration options:
///
/// - `FERRULES_CORRECTION_CACHE_SIZE`: Number of corrections to cache (default: 10000)
/// - `FERRULES_CORRECTION_CACHE_TTL_SECONDS`: Cache TTL in seconds (default: 3600)
/// - `FERRULES_CORRECTION_CONFIDENCE_THRESHOLD`: Minimum confidence for corrections (default: 0.7)
/// - `FERRULES_ENABLE_DICTIONARY_CORRECTIONS`: Enable/disable dictionary corrections (default: true)
/// - `FERRULES_FUZZY_MATCH_THRESHOLD`: Threshold for fuzzy matching (default: 50)
pub const CONFIG_DOCUMENTATION: &str = r#"
Text Correction Configuration Environment Variables:

FERRULES_CORRECTION_CACHE_SIZE          Number of corrections to cache (default: 10000)
FERRULES_CORRECTION_CACHE_TTL_SECONDS   Cache TTL in seconds (default: 3600)
FERRULES_CORRECTION_CONFIDENCE_THRESHOLD Minimum confidence for corrections (default: 0.7)
FERRULES_ENABLE_DICTIONARY_CORRECTIONS  Enable/disable dictionary corrections (default: true)
FERRULES_FUZZY_MATCH_THRESHOLD          Threshold for fuzzy matching (default: 50)
"#;
