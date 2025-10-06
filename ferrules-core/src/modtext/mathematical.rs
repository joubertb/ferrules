//! Mathematical Notation Processing
//!
//! This module handles mathematical text processing and enhancement for formulas.
//! It focuses on mathematical symbol processing, unicode normalization,
//! and formula-specific text enhancements.
//!
//! ## Features
//!
//! - Mathematical Unicode normalization and standardization
//! - Mathematical symbol spacing and formatting
//! - Mathematical accent and diacritic combination
//! - Formula-specific text processing
//!
//! Note: Subscript/superscript detection has been moved to script_notation.rs
//! as it applies to both mathematical formulas AND regular text content.

use crate::debug_print;

// Main process function removed as it's not currently used
// Individual functions are kept as they may be useful for mathematical processing

/// Add appropriate spacing around mathematical symbols
///
/// This function ensures proper readability by adding spaces around mathematical
/// operators and symbols where they don't already exist.
#[allow(dead_code)]
pub fn add_mathematical_spacing(text: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = text.chars().collect();

    let mathematical_operators = [
        '=', '≠', '≈', '≅', '≡', '≢', '≃', '∼', '∝', '+', '−', '±', '∓', '×', '÷', '∗', '∘', '<',
        '>', '≤', '≥', '≼', '≽', '⊑', '⊒', '∈', '∉', '∋', '∌', '⊂', '⊃', '⊆', '⊇', '∪', '∩', '∨',
        '∧', '⊕', '⊗', '⊙', '→', '←', '↔', '⇒', '⇐', '⇔', '↦', '∀', '∃', '∄', '¬', '∴', '∵',
    ];

    for (i, &ch) in chars.iter().enumerate() {
        let needs_space_before = mathematical_operators.contains(&ch)
            && i > 0
            && !chars[i - 1].is_whitespace()
            && chars[i - 1] != '('
            && !result.ends_with(' ');

        let needs_space_after = mathematical_operators.contains(&ch)
            && i < chars.len() - 1
            && !chars[i + 1].is_whitespace()
            && chars[i + 1] != ')';

        if needs_space_before {
            result.push(' ');
        }

        result.push(ch);

        if needs_space_after {
            result.push(' ');
        }
    }

    debug_print!(
        "📏 MATH SPACING: '{}' → '{}'",
        text.chars().take(30).collect::<String>(),
        result.chars().take(30).collect::<String>()
    );

    result
}

/// Handle mathematical summation and product operators with proper spacing
///
/// This function specifically handles ∑, ∏, ∫, and similar operators that need
/// special spacing treatment in mathematical contexts.
#[allow(dead_code)]
pub fn handle_mathematical_operators(text: &str) -> String {
    let mut result = text.to_string();

    // Mathematical operators that need special handling
    let operators = [
        ("∑", " ∑ "), // Summation
        ("∏", " ∏ "), // Product
        ("∫", " ∫ "), // Integral
        ("∮", " ∮ "), // Contour integral
        ("⨀", " ⨀ "), // N-ary circled dot
        ("⨁", " ⨁ "), // N-ary circled plus
        ("⨂", " ⨂ "), // N-ary circled times
        ("⨄", " ⨄ "), // N-ary union
        ("⨅", " ⨅ "), // N-ary intersection
        ("⨆", " ⨆ "), // N-ary square union
    ];

    for (from, to) in operators.iter() {
        // Only replace if not already spaced
        let spaced = format!(" {from} ");
        if !result.contains(&spaced) {
            result = result.replace(from, to);
        }
    }

    // Clean up excessive spacing
    while result.contains("  ") {
        result = result.replace("  ", " ");
    }

    debug_print!(
        "🔣 MATH OPERATORS: '{}' → '{}'",
        text.chars().take(30).collect::<String>(),
        result.chars().take(30).collect::<String>()
    );

    result
}

/// Standardize mathematical notation patterns
///
/// This function applies common mathematical notation standardizations,
/// such as converting alternative representations to preferred forms.
#[allow(dead_code)]
pub fn standardize_mathematical_notation(text: &str) -> String {
    let mut result = text.to_string();

    // Common mathematical notation standardizations
    let standardizations = [
        // Fraction slash variations
        ("⁄", "/"),
        ("∕", "/"),
        // Multiplication variants
        ("⋅", "·"),
        ("∙", "·"),
        // Infinity variants
        ("∞", "∞"),
        // Set membership variants
        ("∊", "∈"),
        ("∍", "∋"),
        // Approximately equal variants
        ("≃", "≈"),
        ("≊", "≈"),
        // Not equal variants
        ("≠", "≠"),
        ("≢", "≠"),
    ];

    for (from, to) in standardizations.iter() {
        result = result.replace(from, to);
    }

    debug_print!(
        "📐 STANDARDIZE: '{}' → '{}'",
        text.chars().take(30).collect::<String>(),
        result.chars().take(30).collect::<String>()
    );

    result
}
