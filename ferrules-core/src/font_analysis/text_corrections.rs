//! Text-level corrections for assembled text
//!
//! This module provides text corrections that work at the assembled text level,
//! complementing the font-level corrections provided by the universal corrector.

/// Fix common mathematical symbol corruptions
///
/// This handles specific mathematical symbol corruption patterns observed
/// in PDF text extraction, particularly bracket and punctuation corruptions.
pub fn fix_math_symbol_corruptions(text: &str) -> String {
    let mut result = text.to_string();

    // Fix "∈ /" or "∈/" to "∉" (not element of)
    result = result.replace("∈ /", "∉");
    result = result.replace("∈/", "∉");

    // Fix "6=" or "6[=]" to "≠" (not equal)
    result = result.replace("6=", "≠");
    result = result.replace("6[=]", "≠");

    // Fix equals sign corruption patterns
    result = result.replace("[=]", " =");
    result = result.replace("< =>", " =");

    // Fix any double spaces around equals
    result = result.replace("  =", " =");
    result = result.replace("=  ", "= ");

    result
}

/// Apply basic character filtering to remove control characters
///
/// This provides UTF-8 cleanup that works regardless of other correction availability.
pub fn filter_control_characters(text: &str) -> String {
    text.chars()
        .filter(|&c| !c.is_control() || c == '\n' || c == '\r' || c == '\t')
        .collect()
}

/// Apply character-level corrections using the default corrector
///
/// This is the main entry point for character corrections from the public API.
pub fn apply_character_corrections(text: &str) -> String {
    // Apply control character corrections first, then filter remaining control characters
    text.chars()
        .filter_map(|c| match c {
            '\u{0002}' => None, // STX control character → remove (line-break hyphenation)
            '\u{0012}' => Some('('), // DC2 control character → opening parenthesis
            '\u{0013}' => Some(')'), // DC3 control character → closing parenthesis
            '\u{0000}' => Some('('), // NULL character → opening parenthesis
            '\u{0001}' => Some(')'), // SOH control character → closing parenthesis
            _ if c.is_control() && c != '\n' && c != '\r' && c != '\t' => None, // Filter other control chars
            _ => Some(c),
        })
        .collect()
}

/// Apply comprehensive text corrections to assembled text
///
/// This combines mathematical symbol corrections with character filtering.
pub fn correct_assembled_text(text: &str) -> String {
    let math_corrected = fix_math_symbol_corruptions(text);
    filter_control_characters(&math_corrected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_math_symbol_corrections() {
        assert_eq!(fix_math_symbol_corruptions("∈ /"), "∉");
        assert_eq!(fix_math_symbol_corruptions("∈/"), "∉");
        assert_eq!(fix_math_symbol_corruptions("6="), "≠");
        assert_eq!(fix_math_symbol_corruptions("6[=]"), "≠");
        assert_eq!(fix_math_symbol_corruptions("[=]"), " =");
        assert_eq!(fix_math_symbol_corruptions("< =>"), " =");
    }

    #[test]
    fn test_control_character_filtering() {
        assert_eq!(filter_control_characters("test\u{0002}text"), "testtext");
        assert_eq!(filter_control_characters("test\ntext"), "test\ntext");
        assert_eq!(filter_control_characters("test\ttext"), "test\ttext");
    }

    #[test]
    fn test_assembled_text_correction() {
        assert_eq!(correct_assembled_text("∈/\u{0002}"), "∉");
        assert_eq!(correct_assembled_text("6=\ntest"), "≠\ntest");
    }
}
