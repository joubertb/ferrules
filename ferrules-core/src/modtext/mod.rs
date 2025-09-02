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
#[allow(dead_code)]
pub fn process_mathematical_notation(spans: &[crate::entities::CharSpan]) -> String {
    eprintln!(
        "🟢 process_mathematical_notation CALLED with {} spans",
        spans.len()
    );
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

/// Process text spans to add all appropriate tags (bold, subscript, superscript, formula)
///
/// This is the new unified entry point for all text processing with composable tags.
/// When the `modtext` feature is disabled, this returns the original text unchanged.
///
/// # Example
/// ```rust
/// use ferrules_core::modtext;
///
/// let enhanced = modtext::add_tags(&char_spans);
/// ```
pub fn add_tags(spans: &[crate::entities::CharSpan]) -> String {
    eprintln!("🏷️ add_tags CALLED with {} spans", spans.len());
    #[cfg(feature = "modtext")]
    {
        mathematical::apply_tags_recursive(spans, 0)
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

/// Format mathematical formula text with appropriate tags
///
/// When the `modtext` feature is enabled, wraps formula text in XML-style tags.
/// When disabled, returns the text unchanged.
/// Note: Subscript/superscript/bold processing is now handled by add_tags() at the Line level.
pub fn format_formula_text(text: &str) -> String {
    eprintln!(
        "📋 FORMULA: format_formula_text called with: {}",
        text.chars().take(100).collect::<String>()
    );

    // Apply mathematical symbol corrections to fix patterns like "6=" → "≠", "∈/" → "∉"
    #[cfg(feature = "correction-engine")]
    let corrected_text = {
        use crate::correction::character::fix_math_symbol_corruptions;
        let fixed = fix_math_symbol_corruptions(text);
        eprintln!(
            "📋 FORMULA CORRECTION: '{}' → '{}'",
            text.chars().take(50).collect::<String>(),
            fixed.chars().take(50).collect::<String>()
        );
        fixed
    };

    #[cfg(not(feature = "correction-engine"))]
    let corrected_text = text.to_string();

    // Clean up substitute characters and apply spacing
    let cleaned_text = corrected_text.replace('\u{001a}', ""); // Remove SUB (substitute) character
    let final_text = add_spacing_after_math_symbols(&cleaned_text);

    #[cfg(feature = "modtext")]
    {
        format!("<formula>{final_text}</formula>")
    }

    #[cfg(not(feature = "modtext"))]
    {
        final_text
    }
}

/// Add spacing around mathematical symbols when no space exists
///
/// This function ensures proper readability by adding spaces before and after mathematical
/// symbols like ∈, ∪, ∩, etc. when they are immediately preceded or followed by text.
fn add_spacing_after_math_symbols(text: &str) -> String {
    let math_symbols = [
        '∈', '∉', '∪', '∩', '∅', '⊂', '⊃', '⊆', '⊇', '∀', '∃', '∄', '∧', '∨', '¬', '→', '←', '↔',
        '⇒', '⇐', '⇔', '∴', '∵', '≡', '≢', '≤', '≥', '≠', '≈', '≅', '≃', '∼', '∝', '±', '∓', '×',
        '÷', '∗', '∘', '√', '∛', '∜', '∞', '∑', '∏', '∫', '∮', '∂', '∇', '△', '∠', '⊥', '∥', '≼',
        '≽', '⊑', '⊒', '⊥', '⊤',
    ];

    let mut result = String::new();
    let chars: Vec<char> = text.chars().collect();

    for i in 0..chars.len() {
        let current_char = chars[i];

        // Check if current character is a mathematical symbol
        if math_symbols.contains(&current_char) {
            // Check if we need space before the symbol
            if i > 0 {
                let prev_char = chars[i - 1];
                let last_result_char = result.chars().last();
                if let Some(last_char) = last_result_char {
                    if !last_char.is_whitespace() && !is_tag_char(prev_char) && prev_char != '(' {
                        result.push(' ');
                        eprintln!("➕ MATH SPACING: Added space before '{current_char}' after '{prev_char}'");
                    }
                }
            }

            result.push(current_char);

            // Check if we need space after the symbol
            if i + 1 < chars.len() {
                let next_char = chars[i + 1];
                if !next_char.is_whitespace()
                    && next_char != ','
                    && next_char != '.'
                    && next_char != ';'
                    && next_char != ')'
                    && !is_tag_char(next_char)
                {
                    result.push(' ');
                    eprintln!(
                        "➕ MATH SPACING: Added space after '{current_char}' before '{next_char}'"
                    );
                }
            }
        } else {
            result.push(current_char);
        }
    }

    result
}

/// Helper function to check if character is part of HTML tag
fn is_tag_char(c: char) -> bool {
    c == '<' || c == '>' || c == '/'
}
