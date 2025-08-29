//! Character-level text corrections
//!
//! This module handles character-level substitutions to fix common
//! PDF extraction corruption issues, particularly from font subset corruption.

use super::traits::CharacterCorrector;
use lazy_static::lazy_static;
use regex::Regex;

// Import the legitimate parenthetical checker from dictionary module
use super::dictionary::SmartCorrector;

/// Default character corrector implementation
///
/// This applies universal character corrections that work across
/// all font types based on common corruption patterns observed
/// in PDF text extraction.
#[derive(Debug, Clone, Default)]
pub struct UniversalCharacterCorrector;

impl CharacterCorrector for UniversalCharacterCorrector {
    fn correct_characters(&self, text: &str) -> String {
        let _original_len = text.len();

        // Check for legitimate parenthetical patterns that should not be corrected
        if SmartCorrector::is_legitimate_parenthetical(text) {
            return text.to_string();
        }

        // For longer text, apply smart correction that preserves legitimate patterns
        if text.len() > 10 {
            return apply_smart_character_corrections(text);
        }

        // Apply UTF-8 corruption fixes first for multi-character text
        let mut corrected = if text.len() > 1 {
            fix_utf8_corruption(text)
        } else {
            text.to_string()
        };

        // Multi-character pattern corrections disabled - only character-level corrections
        // corrected = apply_text_pattern_corrections(&corrected);

        // Only apply single character substitutions if pattern corrections didn't already fix the text
        if corrected == text
            || corrected.contains('(')
            || corrected.contains(')')
            || corrected.contains('\u{0002}')
        {
            corrected = corrected
                .replace(')', "i") // Most common: ) → i
                .replace('(', "h") // Common: ( → h
                .replace('\u{0002}', "i"); // Control character → i
        }

        // Apply Unicode quote fixes AFTER all other processing
        corrected = corrected
            .replace('\u{201C}', "\"") // Left double quotation mark
            .replace('\u{201D}', "\"") // Right double quotation mark
            .replace('\u{2018}', "'") // Left single quotation mark
            .replace('\u{2019}', "'"); // Right single quotation mark

        corrected
    }
}

/// Apply smart character corrections that preserve legitimate patterns within longer text
///
/// This function scans for legitimate parenthetical patterns and protects them
/// while applying corrections to the rest of the text.
fn apply_smart_character_corrections(text: &str) -> String {
    // Find all legitimate parenthetical patterns in the text
    lazy_static! {
        static ref PARENTHETICAL_FINDER: Regex = Regex::new(r"\([A-Z]{2,}\)|\(\d{4}\)|\([a-zA-Z]+\s+et\s+al\.\)|\(e\.g\.\)|\(i\.e\.\)|\(see\s+\w+\)|\(from\s+\w+\)|\([x-z]\+[x-z]\)").unwrap();
    }

    let mut protected_ranges = Vec::new();

    // Find all matches and their positions
    for mat in PARENTHETICAL_FINDER.find_iter(text) {
        let matched_text = mat.as_str();

        // Verify this is actually a legitimate pattern
        if SmartCorrector::is_legitimate_parenthetical(matched_text) {
            protected_ranges.push((mat.start(), mat.end()));
        }
    }

    // Apply character corrections, but skip protected ranges
    let chars: Vec<char> = text.chars().collect();
    let mut corrected_chars = Vec::new();

    let mut byte_pos = 0;
    for &ch in chars.iter() {
        let char_start = byte_pos;
        let char_end = byte_pos + ch.len_utf8();

        // Check if this character is in a protected range
        let is_protected = protected_ranges
            .iter()
            .any(|(start, end)| char_start >= *start && char_end <= *end);

        if is_protected {
            // Keep character as-is
            corrected_chars.push(ch);
        } else {
            // Apply corrections
            match ch {
                ')' => corrected_chars.push('i'),
                '(' => corrected_chars.push('h'),
                '\u{0002}' => corrected_chars.push('i'),
                _ => corrected_chars.push(ch),
            }
        }

        byte_pos = char_end;
    }

    let result: String = corrected_chars.into_iter().collect();

    // Apply other corrections (UTF-8, Unicode quotes) - pattern corrections disabled
    let mut corrected = fix_utf8_corruption(&result);
    // corrected = apply_text_pattern_corrections(&corrected);

    // Apply Unicode quote fixes
    corrected = corrected
        .replace('\u{201C}', "\"")
        .replace('\u{201D}', "\"")
        .replace('\u{2018}', "'")
        .replace('\u{2019}', "'");

    corrected
}

/// Apply character-level corrections using the default corrector
///
/// This is the main entry point for character corrections from the public API.
pub fn apply_character_corrections(text: &str) -> String {
    // Character substitutions disabled to prevent false changes
    // But keep basic UTF-8 control character filtering
    text.chars()
        .filter(|&c| !c.is_control() || c == '\n' || c == '\r' || c == '\t')
        .collect()
}

/// Multi-character pattern corrections have been disabled
/// Only character-level, UTF-8, smart protection, and dictionary corrections remain active
#[allow(dead_code)]
fn apply_text_pattern_corrections(text: &str) -> String {
    // Multi-character pattern corrections disabled - return text unchanged
    text.to_string()
}

/// Fix UTF-8 corruption in multi-character text
///
/// This handles UTF-8 encoding issues that can occur during PDF extraction,
/// including complex UTF-8 sequence reconstruction for mathematical symbols.
fn fix_utf8_corruption(text: &str) -> String {
    if text.is_empty() {
        return text.to_string();
    }

    let mut result = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        let code_point = ch as u32;

        // Look for UTF-8 4-byte sequence starter (0xF0) for mathematical symbols
        if code_point == 0xF0 && i + 3 < chars.len() {
            let byte1 = chars[i] as u8;
            let byte2 = chars[i + 1] as u32;
            let byte3 = chars[i + 2] as u32;
            let byte4 = chars[i + 3] as u32;

            // Check if the next 3 characters form a valid UTF-8 continuation
            if (0x80..=0xBF).contains(&byte2)
                && (0x80..=0xBF).contains(&byte3)
                && (0x80..=0xBF).contains(&byte4)
            {
                // Reconstruct the UTF-8 bytes
                let utf8_bytes = [byte1, byte2 as u8, byte3 as u8, byte4 as u8];

                // Try to decode the UTF-8 sequence
                if let Ok(decoded_str) = std::str::from_utf8(&utf8_bytes) {
                    result.push_str(decoded_str);
                    i += 4; // Skip all 4 characters
                    continue;
                }
            }
        }

        // Look for UTF-8 3-byte sequence starter (0xE2) for mathematical operators
        if code_point == 0xE2 && i + 2 < chars.len() {
            let byte1 = chars[i] as u8;
            let byte2 = chars[i + 1] as u32;
            let byte3 = chars[i + 2] as u32;

            // Check if the next 2 characters form a valid UTF-8 continuation
            if (0x80..=0xBF).contains(&byte2) && (0x80..=0xBF).contains(&byte3) {
                // Reconstruct the UTF-8 bytes
                let utf8_bytes = [byte1, byte2 as u8, byte3 as u8];

                // Try to decode the UTF-8 sequence
                if let Ok(decoded_str) = std::str::from_utf8(&utf8_bytes) {
                    result.push_str(decoded_str);
                    i += 3; // Skip all 3 characters
                    continue;
                }
            }
        }

        // No UTF-8 sequence detected, add character as-is (but filter control characters)
        // Exception: preserve \u{0002} for character correction processing
        if !ch.is_control() || ch == '\n' || ch == '\r' || ch == '\t' || ch == '\u{0002}' {
            result.push(ch);
        }
        i += 1;
    }

    // Apply ligature corruption fixes
    let ligature_fixed = fix_ligature_corruption(&result);

    // Apply mathematical symbol spacing and corruption fixes
    add_math_symbol_spacing(&ligature_fixed)
}

/// Fix ligature corruption using contextual analysis and conservative patterns
///
/// PDFium sometimes incorrectly maps ligature characters to other symbols during
/// text extraction from PDF files. This function uses a conservative approach that:
///
/// 1. Detects only specific symbols known to be ligature corruption (!@#$%^&*)
/// 2. Uses contextual analysis to determine the most likely original ligature
/// 3. Preserves legitimate punctuation like hyphens (-) and periods (.)
///
/// Known corruption patterns:
/// - fl ligature → ! : work!ows → workflows
/// - fi ligature → # : speci#c → specific  
/// - ff ligature → " : e"ective → effective
///
/// The system is conservative to avoid corrupting legitimate text like
/// "long-context", "e-mail", or "state-of-the-art".
fn fix_ligature_corruption(text: &str) -> String {
    // Use lazy_static for one-time regex compilation
    lazy_static! {
        // Pattern to match ligature corruption symbols in various contexts
        // Handles both direct corruption and corruption with separators
        static ref POTENTIAL_CORRUPTION: Regex = Regex::new(r"([a-zA-Z]+)[\s-]*([!@#$%^&*'\x22])([a-zA-Z]+)").unwrap();
        // Pattern to match standalone corruption symbols (like "#t" for "fit")
        static ref STANDALONE_CORRUPTION: Regex = Regex::new(r"\b([!@#$%^&*'\x22])([a-z]+)\b").unwrap();
    }

    let mut result = text.to_string();

    // Fix potential ligature corruption using contextual analysis
    result = POTENTIAL_CORRUPTION
        .replace_all(&result, |caps: &regex::Captures| {
            let prefix = &caps[1];
            let symbol = &caps[2];
            let suffix = &caps[3];

            // Determine most likely ligature based on context and symbol
            let ligature = determine_ligature_from_context(prefix, suffix, symbol);
            format!("{prefix}{ligature}{suffix}")
        })
        .to_string();

    // Fix standalone corruption patterns (like "#t" -> "fit")
    // Simple direct replacement for common patterns
    result = result.replace("#t", "fit");
    result = result.replace("!ows", "flows");
    result = result.replace("\"ective", "ffective");
    result = result.replace("e$- ciently", "efficiently");

    result = STANDALONE_CORRUPTION
        .replace_all(&result, |caps: &regex::Captures| {
            let symbol = &caps[1];
            let suffix = &caps[2];

            // Handle specific standalone patterns
            match (symbol, suffix) {
                ("#", "t") => "fit".to_string(),
                ("#", "rst") => "first".to_string(),
                ("#", "le") => "file".to_string(),
                ("#", "nd") => "find".to_string(),
                ("#", "nal") => "final".to_string(),
                ("#", "ne") => "fine".to_string(),
                ("!", "ow") => "flow".to_string(),
                ("!", "ows") => "flows".to_string(),
                ("\"", "ect") => "fect".to_string(),
                ("\"", "ective") => "fective".to_string(),
                // Default: assume fi ligature for # and fl ligature for !
                ("#", _) => format!("fi{suffix}"),
                ("!", _) => format!("fl{suffix}"),
                ("\"", _) => format!("ff{suffix}"),
                (_, _) => format!("fi{suffix}"), // Default to fi
            }
        })
        .to_string();

    result
}

/// Intelligently determine the most likely ligature based on context
///
/// Uses word patterns, common English combinations, and symbol types to infer
/// the original ligature that was corrupted during PDF text extraction.
fn determine_ligature_from_context(prefix: &str, suffix: &str, symbol: &str) -> &'static str {
    let full_context = format!("{prefix}{suffix}").to_lowercase();

    // Known fl patterns (workflows, overflow, etc.)
    if suffix == "ows"
        || suffix == "ow"
        || full_context.contains("work") && suffix.starts_with("ow")
        || full_context.contains("over") && suffix.starts_with("ow")
    {
        return "fl";
    }

    // Known ff patterns (effective, office, etc.)
    if full_context.contains("e") && suffix.starts_with("ective")
        || full_context.contains("o") && suffix.starts_with("ice")
        || full_context.contains("di") && suffix.starts_with("erent")
        || full_context.contains("sta") && suffix.starts_with("ing")
    {
        return "ff";
    }

    // Known fi patterns - most common ligature
    if suffix.ends_with("ed")
        || suffix.ends_with("es")
        || suffix.ends_with("ng")
        || suffix.ends_with("er")
        || suffix.ends_with("le")
        || suffix.ends_with("al")
        || full_context.starts_with("uni")
        || full_context.starts_with("simpli")
        || full_context.starts_with("identi")
        || full_context.starts_with("speci")
        || full_context.starts_with("bene")
        || full_context.starts_with("signi")
        || full_context.starts_with("classi")
        || full_context.starts_with("certi")
    {
        return "fi";
    }

    // Symbol-based heuristics (conservative - only for known corruption symbols)
    match symbol {
        "!" => {
            // ! often corrupts fl in "workflows" but fi in most other cases
            if suffix == "ows" || suffix.starts_with("ow") {
                "fl"
            } else {
                "fi"
            }
        }
        "#" => "fi",    // # commonly corrupts fi
        "$" => "fi",    // $ can corrupt fi in some cases
        "%" => "fi",    // % can corrupt fi
        "^" => "fi",    // ^ can corrupt fi
        "&" => "ff",    // & can corrupt ff
        "*" => "fi",    // * can corrupt fi
        "\x22" => "ff", // " commonly corrupts ff (e.g., "effectively")
        "'" => "fi",    // ' can corrupt fi in some cases
        _ => "fi",      // Default to fi as it's the most common ligature
    }
}

/// Add spaces around common mathematical symbols and fix corruptions
///
/// This function handles mathematical symbol spacing and corruption fixes
/// that were originally scattered in entities.rs but belong in the correction engine.
pub fn add_math_symbol_spacing(text: &str) -> String {
    let mut result = text.to_string();

    // First, fix common mathematical symbol corruptions
    result = fix_math_symbol_corruptions(&result);

    let math_symbols = [
        "∈", "∉", "⊂", "⊃", "⊆", "⊇", "∪", "∩", "×", "⋅", "∘", "≤", "≥", "≠", "≡", "≈", "∝", "∞",
        "∑", "∏", "∫", "∂", "∇", "△", "∴", "∵", "→", "←", "↔", "⇒", "⇔",
    ];

    for symbol in &math_symbols {
        // Add spaces around the symbol if they're not already there
        let with_spaces = format!(" {symbol} ");
        let patterns_to_replace = [
            (symbol.to_string(), with_spaces.clone()), // symbol with no spaces
            (format!(" {symbol}"), with_spaces.clone()), // symbol with space before only
            (format!("{symbol} "), with_spaces.clone()), // symbol with space after only
        ];

        for (pattern, replacement) in &patterns_to_replace {
            if result.contains(pattern) && !result.contains(&with_spaces) {
                result = result.replace(pattern, replacement);
                break; // Only replace once per symbol to avoid double-spacing
            }
        }
    }

    // Clean up any double spaces that might have been created
    while result.contains("  ") {
        result = result.replace("  ", " ");
    }

    result
}

/// Fix common mathematical symbol corruptions
///
/// This handles specific mathematical symbol corruption patterns observed
/// in PDF text extraction, particularly bracket and punctuation corruptions.
fn fix_math_symbol_corruptions(text: &str) -> String {
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

    // Fix bracket corruption around punctuation
    result = result.replace("otherwise[.]", "otherwise.");
    result = result.replace("[.]", ".");
    result = result.replace("[,]", ",");
    result = result.replace("[;]", ";");
    result = result.replace("[:]", ":");

    // PERFORMANCE FIX: Use lazy_static regexes instead of compiling them every time
    lazy_static! {
        static ref BRACKET_SUBSCRIPT_RE: Regex = Regex::new(r"\[([a-zA-Z])([0-9]+),\]").unwrap();
        static ref BRACKET_UPPER_RE: Regex = Regex::new(r"\[([a-zA-Z])([A-Z]+)\]").unwrap();
        static ref ANGLE_BRACKET_RE: Regex = Regex::new(r"h([^h]+)i").unwrap();
        static ref PARENS_WORD_RE: Regex =
            Regex::new(r"\b([a-zA-Z]+)\(([a-zA-Z]+)\)([a-zA-Z]*)\b").unwrap();
        static ref PARENS_PARTIAL_RE: Regex = Regex::new(r"\b([a-zA-Z]+)\(([a-zA-Z]+)\b").unwrap();
    }

    // Fix misplaced brackets in subscripts like "[e1,]" to "e<[1]>," but skip if already in angle brackets
    // Use custom logic to avoid matching patterns that are already inside <...>

    // Collect match information first to avoid borrowing issues
    let bracket_replacements: Vec<_> = BRACKET_SUBSCRIPT_RE
        .find_iter(&result)
        .map(|m| {
            let start = m.start();
            let should_skip = start > 0 && result.chars().nth(start - 1) == Some('<');
            (start, m.end(), should_skip, m.as_str().to_string())
        })
        .collect();

    // Apply replacements in reverse order to maintain offsets
    for (start, end, should_skip, original) in bracket_replacements.into_iter().rev() {
        if should_skip {
            continue;
        }
        let replacement = BRACKET_SUBSCRIPT_RE.replace(&original, "$1<[$2]>,");
        result.replace_range(start..end, &replacement);
    }

    // Same approach for upper case patterns
    let upper_replacements: Vec<_> = BRACKET_UPPER_RE
        .find_iter(&result)
        .map(|m| {
            let start = m.start();
            let should_skip = start > 0 && result.chars().nth(start - 1) == Some('<');
            (start, m.end(), should_skip, m.as_str().to_string())
        })
        .collect();

    for (start, end, should_skip, original) in upper_replacements.into_iter().rev() {
        if should_skip {
            continue;
        }
        let replacement = BRACKET_UPPER_RE.replace(&original, "$1<[$2]>");
        result.replace_range(start..end, &replacement);
    }

    // Fix angle bracket corruptions like "hn[i], n[j]i" to "(n[i], n[j])"
    result = ANGLE_BRACKET_RE.replace_all(&result, "($1)").to_string();

    // Fix systematic parentheses corruption where '(' and ')' replace 'h'
    // This appears to be a PDF extraction artifact
    result = PARENS_WORD_RE
        .replace_all(&result, |caps: &regex::Captures| {
            let prefix = &caps[1];
            let middle = &caps[2];
            let suffix = &caps[3];
            // Reconstruct word by replacing parentheses with 'h'
            format!("{prefix}h{middle}{suffix}")
        })
        .to_string();

    // Handle cases with missing closing parenthesis
    result = PARENS_PARTIAL_RE
        .replace_all(&result, |caps: &regex::Captures| {
            let prefix = &caps[1];
            let suffix = &caps[2];
            format!("{prefix}h{suffix}")
        })
        .to_string();

    result
}

/// Character corruption mapping for reference
///
/// These are the most common character corruptions observed in PDF extraction:
/// - ')' → 'i': Very common in subset fonts
/// - '(' → 'h': Common in subset fonts  
/// - '\u{0002}' → 'i': Control character corruption
/// - UTF-8 issues: Malformed sequences and control characters
pub const CORRUPTION_MAPPINGS: &[(&str, &str)] = &[(")", "i"), ("(", "h"), ("\u{0002}", "i")];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_universal_character_corrections() {
        let corrector = UniversalCharacterCorrector::default();

        // Test basic corruptions
        assert_eq!(corrector.correct_characters("w)th"), "with");
        assert_eq!(corrector.correct_characters("whic("), "which");
        assert_eq!(corrector.correct_characters("th)s"), "this");

        // Test control character
        assert_eq!(corrector.correct_characters("w\u{0002}th"), "with");

        // Test no change needed
        assert_eq!(corrector.correct_characters("normal text"), "normal text");
    }

    #[test]
    fn test_legitimate_parenthetical_patterns() {
        let corrector = UniversalCharacterCorrector::default();

        // Test legitimate parenthetical patterns that should NOT be corrected
        assert_eq!(corrector.correct_characters("(NLP)"), "(NLP)");
        assert_eq!(corrector.correct_characters("(PDF)"), "(PDF)");
        assert_eq!(corrector.correct_characters("(API)"), "(API)");
        assert_eq!(corrector.correct_characters("(2018)"), "(2018)");
        assert_eq!(corrector.correct_characters("(e.g.)"), "(e.g.)");
        assert_eq!(corrector.correct_characters("(i.e.)"), "(i.e.)");
        assert_eq!(corrector.correct_characters("(see Figure)"), "(see Figure)");
        assert_eq!(
            corrector.correct_characters("(from Wikipedia)"),
            "(from Wikipedia)"
        );
        assert_eq!(
            corrector.correct_characters("(Smith et al.)"),
            "(Smith et al.)"
        );

        // Test that corrupted text still gets fixed
        assert_eq!(
            corrector.correct_characters("w)th (NLP) tasks"),
            "with (NLP) tasks"
        );
    }

    #[test]
    fn test_apply_character_corrections() {
        // Character substitutions are disabled - should return text without control chars
        assert_eq!(apply_character_corrections("be)tween"), "be)tween");
        assert_eq!(apply_character_corrections("normal"), "normal");

        // Test that patterns are unchanged (no substitutions applied)
        assert_eq!(apply_character_corrections("(NLP)"), "(NLP)");
        assert_eq!(apply_character_corrections("(2018)"), "(2018)");

        // Test that control characters are still filtered
        assert_eq!(apply_character_corrections("test\u{0002}text"), "testtext");
    }

    #[test]
    fn test_is_legitimate_parenthetical_pattern() {
        // Test uppercase abbreviations
        assert!(SmartCorrector::is_legitimate_parenthetical("(NLP)"));
        assert!(SmartCorrector::is_legitimate_parenthetical("(PDF)"));
        assert!(SmartCorrector::is_legitimate_parenthetical("(API)"));
        assert!(SmartCorrector::is_legitimate_parenthetical("(HTML)"));

        // Test years
        assert!(SmartCorrector::is_legitimate_parenthetical("(2018)"));
        assert!(SmartCorrector::is_legitimate_parenthetical("(1999)"));

        // Test references
        assert!(SmartCorrector::is_legitimate_parenthetical("(e.g.)"));
        assert!(SmartCorrector::is_legitimate_parenthetical("(i.e.)"));
        assert!(SmartCorrector::is_legitimate_parenthetical("(see Figure)"));
        assert!(SmartCorrector::is_legitimate_parenthetical(
            "(from Wikipedia)"
        ));
        assert!(SmartCorrector::is_legitimate_parenthetical(
            "(Smith et al.)"
        ));

        // Test non-legitimate patterns (these should return false)
        assert!(!SmartCorrector::is_legitimate_parenthetical("w)th"));
        assert!(!SmartCorrector::is_legitimate_parenthetical("(corrupted"));
        assert!(!SmartCorrector::is_legitimate_parenthetical("normal text"));
        assert!(!SmartCorrector::is_legitimate_parenthetical("(a)")); // Too short
    }

    #[test]
    fn test_utf8_corruption_fix() {
        // Test that control characters are filtered
        let input = "normal\u{0001}text\u{0003}here";
        let result = fix_utf8_corruption(input);
        assert_eq!(result, "normaltexthere");

        // Test that valid characters are preserved
        let input = "normal\ntext\there";
        let result = fix_utf8_corruption(input);
        assert_eq!(result, "normal\ntext\there");
    }
}
