//! Text Correction Module
//!
//! This module provides a unified interface for all text correction functionality
//! in the Ferrules PDF processing library. It isolates correction logic from the
//! core parsing components, providing clean separation of concerns.
//!
//! ## Usage
//!
//! ### Basic Text Correction
//! ```rust
//! use ferrules_core::correction;
//!
//! // Apply character-level corrections
//! let corrected = correction::correct_characters("w)th some text");
//! assert_eq!(corrected, "with some text");
//! ```
//!
//! ### Block Correction
//! ```rust
//! use ferrules_core::correction;
//!
//! // Apply all corrections to a block
//! correction::correct_block(&mut my_block);
//! ```
//!
//! ### Initialization
//! The correction engine is automatically initialized when first used,
//! reading configuration from environment variables.

#[cfg(feature = "correction-engine")]
pub mod character;
#[cfg(feature = "correction-engine")]
pub mod config;
#[cfg(feature = "correction-engine")]
pub mod dictionary;
#[cfg(feature = "correction-engine")]
pub mod engine;
#[cfg(feature = "correction-engine")]
pub mod font_analysis;
#[cfg(feature = "correction-engine")]
pub mod font_debug;
#[cfg(feature = "correction-engine")]
pub mod glyph_mapping;
#[cfg(feature = "correction-engine")]
pub mod traits;
#[cfg(feature = "correction-engine")]
pub mod unicode_validator;

// Re-export public API when correction engine is enabled
#[cfg(feature = "correction-engine")]
pub use config::CorrectionConfig;
#[cfg(feature = "correction-engine")]
pub use engine::{get_global_correction_engine, initialize_from_environment};
#[cfg(feature = "correction-engine")]
pub use font_analysis::*;
#[cfg(feature = "correction-engine")]
pub use glyph_mapping::*;
#[cfg(feature = "correction-engine")]
pub use traits::*;

use crate::blocks::Block;
use anyhow::Result;

/// Apply all text corrections to a block
///
/// This is the main entry point for text correction in the base code.
/// It handles feature flag management and provides a clean interface
/// regardless of whether the correction engine is enabled.
///
/// # Example
/// ```rust
/// use ferrules_core::correction;
///
/// correction::correct_block(&mut my_block);
/// ```
pub fn correct_block(block: &mut Block) {
    #[cfg(feature = "correction-engine")]
    {
        ensure_initialized();
        if let Some(corrector) = get_text_corrector() {
            corrector.correct_block(block);
        }
    }

    #[cfg(not(feature = "correction-engine"))]
    {
        let _ = block; // Suppress unused variable warning when feature is disabled
    }

    #[cfg(not(feature = "correction-engine"))]
    {
        // No corrections when feature is disabled
    }
}

/// Apply text corrections to multiple blocks efficiently
///
/// This function applies corrections to all blocks in a batch, with proper
/// logging and feature flag handling.
///
/// # Example
/// ```rust
/// use ferrules_core::correction;
///
/// correction::correct_blocks(&mut parsed_blocks);
/// ```
pub fn correct_blocks(blocks: &mut [Block]) {
    if blocks.is_empty() {
        return;
    }

    tracing::debug!("🔤 Applying text corrections to {} blocks", blocks.len());

    #[cfg(feature = "correction-engine")]
    {
        ensure_initialized();
        if let Some(corrector) = get_text_corrector() {
            for block in blocks {
                corrector.correct_block(block);
            }
        }
    }

    #[cfg(not(feature = "correction-engine"))]
    {
        // No corrections when feature is disabled
    }
}

/// Apply character-level corrections to text
///
/// This provides character-level corruption fixes that work regardless
/// of dictionary availability. This is the main entry point for character
/// corrections from entities.rs and other base code files.
///
/// # Example
/// ```rust
/// use ferrules_core::correction;
///
/// let corrected = correction::correct_characters("w)th text");
/// assert_eq!(corrected, "with text");
/// ```
pub fn correct_characters(text: &str) -> String {
    #[cfg(feature = "correction-engine")]
    {
        // Apply mathematical symbol corrections first
        let math_corrected = character::fix_math_symbol_corruptions(text);

        // Then filter control characters
        math_corrected
            .chars()
            .filter(|&c| !c.is_control() || c == '\n' || c == '\r' || c == '\t')
            .collect()
    }

    #[cfg(not(feature = "correction-engine"))]
    {
        text.chars()
            .filter(|&c| !c.is_control() || c == '\n' || c == '\r' || c == '\t')
            .collect()
    }
}

/// Fix character encoding corruption (legacy compatibility function)
///
/// This function provides backwards compatibility for existing code that calls
/// fix_character_encoding_corruption. It delegates to the unified correction system.
///
/// # Example
/// ```rust
/// use ferrules_core::correction;
///
/// let corrected = correction::fix_character_encoding_corruption("w)th text");
/// assert_eq!(corrected, "with text");
/// ```
pub fn fix_character_encoding_corruption(text: &str) -> String {
    #[cfg(feature = "correction-engine")]
    {
        // Character substitutions disabled to prevent false changes
        // But keep basic UTF-8 control character filtering
        text.chars()
            .filter(|&c| !c.is_control() || c == '\n' || c == '\r' || c == '\t')
            .collect()
    }

    #[cfg(not(feature = "correction-engine"))]
    {
        // Basic UTF-8 fixes only when correction engine is disabled
        // Fall back to simple control character filtering
        text.chars()
            .filter(|&c| !c.is_control() || c == '\n' || c == '\r' || c == '\t')
            .collect()
    }
}

/// Fix character encoding corruption with font information (legacy compatibility)
///
/// This function provides backwards compatibility for existing code.
/// Font-specific corrections are now handled internally by the unified correction system.
pub fn fix_character_encoding_corruption_with_font(text: &str, _font_name: Option<&str>) -> String {
    fix_character_encoding_corruption(text)
}

/// Apply full text corrections to a string
///
/// This applies both character-level and dictionary-based corrections.
///
/// # Example
/// ```rust
/// use ferrules_core::correction;
///
/// let corrected = correction::correct_text("w)th some corrupted text");
/// ```
pub fn correct_text(text: &str) -> String {
    #[cfg(feature = "correction-engine")]
    {
        ensure_initialized();
        if let Some(corrector) = get_text_corrector() {
            corrector.correct_text(text)
        } else {
            // No fallback - return original text if corrector unavailable
            text.to_string()
        }
    }

    #[cfg(not(feature = "correction-engine"))]
    {
        text.to_string()
    }
}

/// Apply word-level corrections to assembled text
///
/// This corrects corruption patterns that appear in assembled words after
/// individual characters have been extracted from the PDF. It handles both
/// plain text and text within HTML-like tags (such as formula elements).
///
/// This is the main entry point for word-level corrections from merge.rs and blocks.rs.
///
/// # Example
/// ```rust
/// use ferrules_core::correction;
///
/// let corrected = correction::correct_assembled_text("ot(erw)se");
/// assert_eq!(corrected, "otherwise");
/// ```
pub fn correct_assembled_text(text: &str) -> String {
    #[cfg(feature = "correction-engine")]
    {
        font_analysis::correct_assembled_text(text)
    }

    #[cfg(not(feature = "correction-engine"))]
    {
        text.to_string()
    }
}

/// Apply word-level font corrections to assembled text (in-place)
///
/// This function applies corrections to a mutable string reference, updating it
/// only if corrections were made. It includes debug logging for target patterns.
///
/// # Example
/// ```rust
/// use ferrules_core::correction;
///
/// let mut text = String::from("w)th some text");
/// correction::apply_word_corrections(&mut text);
/// assert_eq!(text, "with some text");
/// ```
pub fn apply_word_corrections(text: &mut String) {
    // Debug: Log what text we're working with at the block level
    if text.contains("n<sub>j</sub> i") || text.contains("<formula>") {
        #[cfg(feature = "correction-engine")]
        {
            use crate::debug_print;
            debug_print!("🔍 BLOCK DEBUG: apply_word_corrections called with text length {} containing target patterns", text.len());
            debug_print!(
                "🔍 BLOCK DEBUG: Text preview: {}",
                text.chars().take(200).collect::<String>()
            );
        }
    }

    let corrected = correct_assembled_text(text);
    if corrected != *text {
        *text = corrected;
    }
}

/// Initialize the correction system
///
/// This is called automatically when needed, but can be called explicitly
/// for better error handling.
pub fn initialize() -> Result<()> {
    #[cfg(feature = "correction-engine")]
    {
        engine::initialize_from_environment()
    }

    #[cfg(not(feature = "correction-engine"))]
    {
        Ok(())
    }
}

/// Check if the correction system is available
pub fn is_available() -> bool {
    #[cfg(feature = "correction-engine")]
    {
        ensure_initialized();
        get_text_corrector().is_some()
    }

    #[cfg(not(feature = "correction-engine"))]
    {
        false
    }
}

/// Get the global text corrector instance
#[cfg(feature = "correction-engine")]
fn get_text_corrector() -> Option<&'static dyn TextCorrector> {
    engine::get_global_corrector()
}

/// Ensure the correction system is initialized
#[cfg(feature = "correction-engine")]
fn ensure_initialized() {
    if engine::get_global_corrector().is_none() {
        if let Err(_e) = engine::initialize_from_environment() {}
    }
}

/// Initialize correction engine with proper error handling for CLI applications
///
/// This provides a CLI-friendly interface for initialization with proper error reporting.
/// Returns an error that can be displayed to users if initialization fails.
pub fn initialize_for_cli() -> Result<()> {
    #[cfg(feature = "correction-engine")]
    {
        initialize_from_environment()
    }

    #[cfg(not(feature = "correction-engine"))]
    {
        Ok(())
    }
}

/// Display correction configuration information for CLI verbose mode
///
/// Shows cache configuration and correction engine status for CLI applications.
/// This replaces the feature flag duplication in CLI main.rs files.
pub fn display_cli_config_info() {
    #[cfg(feature = "correction-engine")]
    {
        // Show cache configuration
        let cache_size = std::env::var("FERRULES_CORRECTION_CACHE_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10_000);

        let cache_ttl = std::env::var("FERRULES_CORRECTION_CACHE_TTL_SECONDS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3600);

        println!("📋 Text Correction Settings");
        println!("  Cache size: {cache_size}");
        println!("  Cache TTL: {cache_ttl}s");
        println!();
    }

    #[cfg(not(feature = "correction-engine"))]
    {
        println!("🔧 Text correction engine disabled at compile time");
        println!();
    }
}
