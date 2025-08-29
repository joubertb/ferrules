//! Text Modification Module
//!
//! This module provides text modification functionality for enhanced readability
//! and improved text processing. It handles mathematical notation, subscripts, 
//! superscripts, and other text enhancements that improve accessibility and
//! usability across various applications.
//!
//! ## Usage
//!
//! ### Mathematical Notation Processing
//! ```rust
//! use ferrules_core::modtext;
//! 
//! // Process mathematical subscripts and superscripts for better readability
//! let enhanced = modtext::process_mathematical_notation(&char_spans);
//! ```
//!
//! ### Feature Flag
//! This module is gated behind the `modtext` feature flag. When disabled,
//! all functions return unmodified text for minimal impact on performance.

#[cfg(feature = "modtext")]
pub mod mathematical;

/// Process text spans for mathematical notation enhancement
/// 
/// This is the main entry point for mathematical text processing.
/// When the `modtext` feature is disabled, this returns the original text unchanged.
/// 
/// # Example
/// ```rust
/// use ferrules_core::modtext;
/// 
/// let enhanced = modtext::process_mathematical_notation(&char_spans);
/// ```
pub fn process_mathematical_notation(spans: &[crate::entities::CharSpan]) -> String {
    #[cfg(feature = "modtext")]
    {
        mathematical::detect_script_notation(spans)
    }
    
    #[cfg(not(feature = "modtext"))]
    {
        // When feature is disabled, just concatenate the text without processing
        spans.iter().map(|s| s.text.as_str()).collect()
    }
}

/// Detect inline subscript patterns within text
/// 
/// When the `modtext` feature is disabled, this returns None.
#[allow(dead_code)]
pub fn detect_inline_subscript_pattern(_text: &str) -> Option<(String, String)> {
    #[cfg(feature = "modtext")]
    {
        mathematical::detect_inline_subscript(_text)
    }
    
    #[cfg(not(feature = "modtext"))]
    {
        None
    }
}