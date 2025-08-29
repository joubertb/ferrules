//! Core traits for text correction system

use crate::blocks::Block;

/// Main trait for text correction functionality
///
/// This trait provides a unified interface for applying all types of
/// corrections to text content in PDF documents.
pub trait TextCorrector: Send + Sync {
    /// Apply all corrections to a text string
    ///
    /// This includes both character-level corrections (fixing corruption)
    /// and dictionary-based corrections (spell checking and word validation).
    fn correct_text(&self, text: &str) -> String;

    /// Apply corrections to a Block structure
    ///
    /// This handles the block type-specific logic for applying corrections
    /// to different types of content (TextBlock, Header, Footer, etc.).
    fn correct_block(&self, block: &mut Block);
}

/// Trait for character-level text corrections
///
/// This handles low-level character substitutions to fix common
/// PDF extraction corruption issues.
pub trait CharacterCorrector: Send + Sync {
    /// Apply character-level corrections to text
    ///
    /// Examples of corrections:
    /// - ')' → 'i' (common font subset corruption)
    /// - '(' → 'h' (common font subset corruption)
    /// - Control characters → proper characters
    fn correct_characters(&self, text: &str) -> String;
}

/// Trait for dictionary-based text corrections
///
/// This handles word-level corrections using dictionary validation
/// and spell checking.
pub trait DictionaryCorrector: Send + Sync {
    /// Apply dictionary-based corrections to text
    ///
    /// This performs:
    /// - Word validation against dictionary
    /// - Spell checking and suggestions
    /// - Context-aware corrections
    fn correct_words(&self, text: &str) -> String;

    /// Check if the corrector is available and ready
    fn is_available(&self) -> bool;
}

/// Configuration trait for correction behavior
pub trait CorrectionConfiguration {
    /// Get cache size for correction results
    fn cache_size(&self) -> u64;

    /// Get cache TTL in seconds
    fn cache_ttl_seconds(&self) -> u64;

    /// Get confidence threshold for corrections
    fn confidence_threshold(&self) -> f64;

    /// Check if dictionary corrections are enabled
    fn dictionary_corrections_enabled(&self) -> bool;
}
