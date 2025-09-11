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

/// Normalize mathematical Unicode variants to standard ASCII where appropriate
///
/// This function converts mathematical Unicode variants back to their ASCII equivalents
/// for improved compatibility and readability in text-to-speech applications.
pub fn normalize_mathematical_unicode(text: &str) -> String {
    let mut result = text.to_string();

    // Mathematical styled variants → ASCII
    let replacements = [
        // Mathematical Bold
        ('𝐀', 'A'),
        ('𝐁', 'B'),
        ('𝐂', 'C'),
        ('𝐃', 'D'),
        ('𝐄', 'E'),
        ('𝐅', 'F'),
        ('𝐆', 'G'),
        ('𝐇', 'H'),
        ('𝐈', 'I'),
        ('𝐉', 'J'),
        ('𝐊', 'K'),
        ('𝐋', 'L'),
        ('𝐌', 'M'),
        ('𝐍', 'N'),
        ('𝐎', 'O'),
        ('𝐏', 'P'),
        ('𝐐', 'Q'),
        ('𝐑', 'R'),
        ('𝐒', 'S'),
        ('𝐓', 'T'),
        ('𝐔', 'U'),
        ('𝐕', 'V'),
        ('𝐖', 'W'),
        ('𝐗', 'X'),
        ('𝐘', 'Y'),
        ('𝐙', 'Z'),
        ('𝐚', 'a'),
        ('𝐛', 'b'),
        ('𝐜', 'c'),
        ('𝐝', 'd'),
        ('𝐞', 'e'),
        ('𝐟', 'f'),
        ('𝐠', 'g'),
        ('𝐡', 'h'),
        ('𝐢', 'i'),
        ('𝐣', 'j'),
        ('𝐤', 'k'),
        ('𝐥', 'l'),
        ('𝐦', 'm'),
        ('𝐧', 'n'),
        ('𝐨', 'o'),
        ('𝐩', 'p'),
        ('𝐪', 'q'),
        ('𝐫', 'r'),
        ('𝐬', 's'),
        ('𝐭', 't'),
        ('𝐮', 'u'),
        ('𝐯', 'v'),
        ('𝐰', 'w'),
        ('𝐱', 'x'),
        ('𝐲', 'y'),
        ('𝐳', 'z'),
        // Mathematical Italic
        ('𝐴', 'A'),
        ('𝐵', 'B'),
        ('𝐶', 'C'),
        ('𝐷', 'D'),
        ('𝐸', 'E'),
        ('𝐹', 'F'),
        ('𝐺', 'G'),
        ('𝐻', 'H'),
        ('𝐼', 'I'),
        ('𝐽', 'J'),
        ('𝐾', 'K'),
        ('𝐿', 'L'),
        ('𝑀', 'M'),
        ('𝑁', 'N'),
        ('𝑂', 'O'),
        ('𝑃', 'P'),
        ('𝑄', 'Q'),
        ('𝑅', 'R'),
        ('𝑆', 'S'),
        ('𝑇', 'T'),
        ('𝑈', 'U'),
        ('𝑉', 'V'),
        ('𝑊', 'W'),
        ('𝑋', 'X'),
        ('𝑌', 'Y'),
        ('𝑍', 'Z'),
        ('𝑎', 'a'),
        ('𝑏', 'b'),
        ('𝑐', 'c'),
        ('𝑑', 'd'),
        ('𝑒', 'e'),
        ('𝑓', 'f'),
        ('𝑔', 'g'),
        ('𝘩', 'h'),
        ('𝑖', 'i'),
        ('𝑗', 'j'),
        ('𝑘', 'k'),
        ('𝑙', 'l'),
        ('𝑚', 'm'),
        ('𝑛', 'n'),
        ('𝑜', 'o'),
        ('𝑝', 'p'),
        ('𝑞', 'q'),
        ('𝑟', 'r'),
        ('𝑠', 's'),
        ('𝑡', 't'),
        ('𝑢', 'u'),
        ('𝑣', 'v'),
        ('𝑤', 'w'),
        ('𝑥', 'x'),
        ('𝑦', 'y'),
        ('𝑧', 'z'),
    ];

    for (from, to) in replacements.iter() {
        result = result.replace(*from, &to.to_string());
    }

    debug_print!(
        "🔄 UNICODE NORMALIZE: '{}' → '{}'",
        text.chars().take(30).collect::<String>(),
        result.chars().take(30).collect::<String>()
    );

    result
}

/// Combine mathematical diacritical marks with their base characters
///
/// This function processes sequences of base characters followed by combining
/// diacritical marks and combines them into single characters where possible.
pub fn combine_mathematical_accents(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        result.push(ch);

        // Check for combining diacritical marks
        while let Some(&next_ch) = chars.peek() {
            match next_ch {
                '\u{0300}'..='\u{036F}' | // Combining Diacritical Marks
                '\u{1AB0}'..='\u{1AFF}' | // Combining Diacritical Marks Extended
                '\u{1DC0}'..='\u{1DFF}' | // Combining Diacritical Marks Supplement
                '\u{20D0}'..='\u{20FF}' => { // Combining Diacritical Marks for Symbols
                    result.push(chars.next().unwrap());
                }
                _ => break,
            }
        }
    }

    debug_print!(
        "🔠 COMBINE ACCENTS: '{}' → '{}'",
        text.chars().take(30).collect::<String>(),
        result.chars().take(30).collect::<String>()
    );

    result
}

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
