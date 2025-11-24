//! Text-level corrections for assembled text
//!
//! This module provides text corrections that work at the assembled text level,
//! complementing the font-level corrections provided by the universal corrector.

/// Fix common character positioning corruptions
///
/// This handles character insertion/positioning issues where characters from
/// nearby text get incorrectly inserted into words during PDF text extraction.
/// Uses sophisticated dictionary-based validation with caching and fuzzy matching.
pub fn fix_character_positioning_corruptions(text: &str) -> String {
    #[cfg(feature = "correction-engine")]
    {
        use crate::font_analysis::dictionary::SmartCorrector;

        // Skip dictionary correction for HTML-tagged text to avoid corrupting markup
        if text.contains("<sub>") || text.contains("<sup>") || text.contains("<formula>") {
            return text.to_string();
        }

        // Use global corrector instance for efficiency
        let corrector = SmartCorrector::global()
            .expect("Failed to initialize SmartCorrector - dictionary files missing or corrupted");

        corrector.correct_text_sync(text)
    }

    #[cfg(not(feature = "correction-engine"))]
    {
        panic!("Dictionary correction is not available - correction-engine feature is disabled");
    }
}

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

    // Convert em-dashes to double hyphens with spaces for better readability and TTS compatibility
    // Handle both spaced and unspaced em-dashes to avoid double spacing
    // NOTE: Only convert em-dashes (—), preserve en-dashes (–) for ranges like "37–50%"
    result = result.replace(" — ", " -- "); // Already spaced em-dash
    result = result.replace("—", " -- "); // Unspaced em-dash

    // Clean up any potential double spaces created
    result = result.replace("  --  ", " -- ");
    result = result.replace("  -- ", " -- ");
    result = result.replace(" --  ", " -- ");

    result
}

/// Apply basic character filtering to remove control characters
///
/// This provides UTF-8 cleanup that works regardless of other correction availability.
/// U+0002 (STX) is converted to hyphen as it's used by some PDFs for line-break hyphenation.
pub fn filter_control_characters(text: &str) -> String {
    text.chars()
        .map(|c| {
            // U+0002 (STX - Start of Text) is used by some PDFs to mark hyphenated line breaks
            // Convert it to a regular hyphen so downstream logic can join the words
            if c == '\u{0002}' {
                '-'
            } else {
                c
            }
        })
        .filter(|&c| !c.is_control() || c == '\n' || c == '\r' || c == '\t' || c == '-')
        .collect()
}

/// Apply character-level corrections using the default corrector
///
/// This is the main entry point for character corrections from the public API.
pub fn apply_character_corrections(text: &str) -> String {
    // Apply control character corrections first, then filter remaining control characters
    text.chars()
        .filter_map(|c| match c {
            '\u{0002}' => Some('-'), // STX control character → hyphen (line-break hyphenation marker)
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
/// This combines mathematical symbol corrections, character positioning fixes, and character filtering.
/// Ligature corrections are now handled at the font analysis level via Adobe Glyph List.
pub fn correct_assembled_text(text: &str) -> String {
    // Apply math symbol corrections first, as they should work on both HTML and plain text
    let math_corrected = fix_math_symbol_corruptions(text);

    // Apply character positioning corrections (may skip HTML content for dictionary corrections)
    let positioning_corrected = fix_character_positioning_corruptions(&math_corrected);

    // Apply final character filtering
    filter_control_characters(&positioning_corrected)
}

/// Apply dictionary corrections directly to spans (modifies span text in place)
///
/// This fixes character insertion issues like 'sysfitems' → 'systems' at the span level
/// before HTML processing occurs, ensuring corrections are preserved.
pub fn correct_spans_with_dictionary(spans: &mut [crate::entities::CharSpan]) {
    #[cfg(feature = "correction-engine")]
    {
        use crate::font_analysis::dictionary::SmartCorrector;

        // Use global corrector instance for efficiency
        if let Ok(corrector) = SmartCorrector::global() {
            corrector.correct_spans_sync(spans);
        } else {
            eprintln!("Warning: Failed to initialize SmartCorrector - dictionary files missing or corrupted");
        }
    }

    #[cfg(not(feature = "correction-engine"))]
    {
        // No-op when correction engine is disabled
        let _ = spans;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_character_positioning_corrections() {
        // Test the specific case from the PDF
        assert_eq!(
            fix_character_positioning_corruptions("sysfitems"),
            "systems"
        );
        assert_eq!(
            fix_character_positioning_corruptions("processing sysfitems are"),
            "processing systems are"
        );

        // Test that valid words are not changed
        assert_eq!(fix_character_positioning_corruptions("systems"), "systems");
        assert_eq!(
            fix_character_positioning_corruptions("information processing"),
            "information processing"
        );

        // Test that unknown words are left unchanged
        assert_eq!(fix_character_positioning_corruptions("unknown"), "unknown");
    }

    #[test]
    fn test_math_symbol_corrections() {
        assert_eq!(fix_math_symbol_corruptions("∈ /"), "∉");
        assert_eq!(fix_math_symbol_corruptions("∈/"), "∉");
        assert_eq!(fix_math_symbol_corruptions("6="), "≠");
        assert_eq!(fix_math_symbol_corruptions("6[=]"), "≠");
        assert_eq!(fix_math_symbol_corruptions("[=]"), " =");
        assert_eq!(fix_math_symbol_corruptions("< =>"), " =");
        // Test em-dash conversions for better TTS compatibility
        assert_eq!(
            fix_math_symbol_corruptions("categories—direct jailbreak—into"),
            "categories -- direct jailbreak -- into"
        );
        // En-dashes should be preserved (used for ranges like "37–50%")
        assert_eq!(fix_math_symbol_corruptions("en-dash–test"), "en-dash–test");
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
        // Dictionary corrector normalizes whitespace (newlines become spaces)
        assert_eq!(correct_assembled_text("6=\ntest"), "≠ test");
        assert_eq!(correct_assembled_text("sysfitems\u{0002}"), "systems");
        // Note: Ligature corrections are now handled at the font analysis level via Adobe Glyph List
    }
}
