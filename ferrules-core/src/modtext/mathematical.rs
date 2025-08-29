//! Mathematical Notation Processing
//!
//! This module handles the detection and conversion of mathematical subscripts and superscripts
//! to bracket notation for improved readability and accessibility.
//!
//! ## Features
//!
//! - Detects subscripts and superscripts based on font size and position
//! - Converts to standardized bracket notation: "x₂" → "x<[2]>", "n^i" → "n^<[i]>"
//! - Handles inline subscript patterns within single text spans
//! - Processes mathematical symbols and spacing

use lazy_static::lazy_static;
use regex::Regex;
use crate::entities::CharSpan;

/// Detect and convert subscripts and superscripts to bracket notation
///
/// Analyzes character positioning and font sizes to identify mathematical subscripts
/// and superscripts, converting them to standardized bracket notation for better readability:
/// - Subscripts: "ni" → "n<[i]>", "LossMSP" → "Loss<[MSP]>"  
/// - Superscripts: "x²" → "x^<[2]>", "a³" → "a^<[3]>"
///
/// Detection criteria:
/// - Vertical position offset (y-coordinate difference)
/// - Font size difference between base and script characters
/// - Horizontal proximity for character grouping
pub(crate) fn detect_script_notation(spans: &[CharSpan]) -> String {
    if spans.is_empty() {
        return String::new();
    }

    // Configuration thresholds - made more lenient
    const SUBSCRIPT_Y_THRESHOLD: f32 = 1.0; // Reduced from 2.0 - more sensitive to small position changes
    const SUPERSCRIPT_Y_THRESHOLD: f32 = 1.0; // Reduced from 2.0
    const FONT_SIZE_RATIO_THRESHOLD: f32 = 0.9; // Increased from 0.85 - less strict font size requirement
    const HORIZONTAL_PROXIMITY: f32 = 20.0; // Increased from 10.0 - allow wider gaps

    let mut result = String::new();
    let mut i = 0;

    while i < spans.len() {
        let base_span = &spans[i];
        let mut script_chars = Vec::new();
        let mut script_type = None; // None, Some("sub"), Some("sup")

        // Also check for common subscript patterns in single spans
        if let Some((base_part, script_part)) = detect_inline_subscript(&base_span.text) {
            result.push_str(&format!("{base_part}<[{script_part}]>"));
            i += 1;
            continue;
        }

        // Look ahead for potential script characters
        let mut j = i + 1;
        while j < spans.len() {
            let next_span = &spans[j];

            // Check horizontal proximity - use previous span for proximity, not base
            let prev_span = if j > i + 1 { &spans[j - 1] } else { base_span };
            if next_span.bbox.x0 - prev_span.bbox.x1 > HORIZONTAL_PROXIMITY {
                break;
            }

            // Determine if this is a subscript or superscript relative to base character
            let y_diff = next_span.bbox.y0 - base_span.bbox.y0;
            let font_ratio = next_span.font_size / base_span.font_size;

            // Check if the character is just whitespace or empty - skip these for script detection
            let is_whitespace_only = next_span.text.trim().is_empty();

            // More lenient detection for subscripts, but stricter for superscripts
            let is_subscript = !is_whitespace_only
                && (
                    (y_diff > SUBSCRIPT_Y_THRESHOLD && font_ratio <= FONT_SIZE_RATIO_THRESHOLD)
                        || (y_diff > 0.5 && font_ratio <= 1.0)
                    // Even more lenient for slight position changes
                );
            // Be much more conservative with superscript detection to avoid false positives
            let is_superscript = !is_whitespace_only
                && y_diff < -SUPERSCRIPT_Y_THRESHOLD
                && font_ratio <= FONT_SIZE_RATIO_THRESHOLD
                && font_ratio < 0.8; // Require significant font size difference for superscripts

            if is_subscript {
                if script_type.is_none() {
                    script_type = Some("sub");
                } else if script_type != Some("sub") {
                    break; // Mixed script types, stop grouping
                }
                script_chars.push(&next_span.text);
                j += 1;
            } else if is_superscript {
                if script_type.is_none() {
                    script_type = Some("sup");
                } else if script_type != Some("sup") {
                    break; // Mixed script types, stop grouping
                }
                script_chars.push(&next_span.text);
                j += 1;
            } else {
                break; // Not a script character
            }
        }

        // Generate output based on detected script pattern
        if !script_chars.is_empty() {
            let script_text: String = script_chars.iter().map(|s| s.as_str()).collect();
            // Only apply bracket notation if script text is not empty or just whitespace
            let trimmed_script = script_text.trim();
            if !trimmed_script.is_empty() {
                match script_type {
                    Some("sub") => {
                        // Check if the script text itself contains inline subscripts
                        if let Some((inner_base, inner_script)) =
                            detect_inline_subscript(trimmed_script)
                        {
                            let formatted = format!(
                                "{}{}<[{}]>",
                                base_span.text.trim_end(),
                                inner_base,
                                inner_script
                            );

                            result.push_str(&formatted);
                        } else {
                            let formatted =
                                format!("{}<[{}]>", base_span.text.trim_end(), trimmed_script);

                            result.push_str(&formatted);
                        }
                    }
                    Some("sup") => {
                        // Check if the script text itself contains inline subscripts
                        if let Some((inner_base, inner_script)) =
                            detect_inline_subscript(trimmed_script)
                        {
                            result.push_str(&format!(
                                "{}{}^<[{}]>",
                                base_span.text.trim_end(),
                                inner_base,
                                inner_script
                            ));
                        } else {
                            result.push_str(&format!(
                                "{}^<[{}]>",
                                base_span.text.trim_end(),
                                trimmed_script
                            ));
                        }
                    }
                    _ => {
                        result.push_str(&base_span.text);
                    }
                }
            } else {
                // If script text is empty/whitespace, treat as regular text
                result.push_str(&base_span.text);
                for script_char in &script_chars {
                    result.push_str(script_char);
                }
            }
            i = j; // Skip processed script characters
        } else {
            result.push_str(&base_span.text);
            i += 1;
        }
    }

    result
}

/// Detect common subscript patterns within a single text span
/// 
/// This function identifies mathematical subscript patterns that appear within
/// a single character span, such as "xi", "n0", "tLT", etc.
pub(crate) fn detect_inline_subscript(text: &str) -> Option<(String, String)> {
    // Only handle very specific mathematical subscript patterns
    // Be conservative to avoid breaking regular words

    // Strip trailing comma or punctuation that might interfere with subscript detection
    let clean_text = text.trim_end_matches(',').trim_end_matches('.').trim();

    // PERFORMANCE FIX: Use lazy_static regexes instead of compiling them every time
    lazy_static! {
        // Match patterns like "ei,j", "x1,2" but NOT patterns with trailing comma like "t1,"
        static ref COMMA_SUBSCRIPT_RE: Regex =
            Regex::new(r"^([a-z])([ij],[ij]|[ij],[0-9]|[0-9],[ij]|[0-9],[0-9])$").unwrap();
        static ref LETTER_SUBSCRIPT_RE: Regex = Regex::new(r"^([nxyzehtr])([ij])$").unwrap();
        static ref DIGIT_SUBSCRIPT_RE: Regex = Regex::new(r"^([nxyzehtr])([0-9])$").unwrap();
        // Add pattern for multi-character subscripts like tLT, tLM, etc.
        static ref MULTI_CHAR_SUBSCRIPT_RE: Regex = Regex::new(r"^([a-z])([A-Z]{2,})$").unwrap();
    }

    // Handle comma-separated subscripts like "ei,j", "xi,j", etc.
    if let Some(caps) = COMMA_SUBSCRIPT_RE.captures(clean_text) {
        return Some((caps[1].to_string(), caps[2].to_string()));
    }

    // Handle common single-letter mathematical subscripts: ni, nj, xi, xj, etc.
    if let Some(caps) = LETTER_SUBSCRIPT_RE.captures(clean_text) {
        return Some((caps[1].to_string(), caps[2].to_string()));
    }

    // Handle patterns like "n0", "n1", "x0", "x1" etc (single letter + single digit)
    if let Some(caps) = DIGIT_SUBSCRIPT_RE.captures(clean_text) {
        return Some((caps[1].to_string(), caps[2].to_string()));
    }

    // Handle multi-character subscripts like "tLT", "cLC", etc.
    if let Some(caps) = MULTI_CHAR_SUBSCRIPT_RE.captures(clean_text) {
        return Some((caps[1].to_string(), caps[2].to_string()));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{BBox, CharSpan};

    #[test]
    fn test_detect_script_notation_subscript() {
        // Test subscript detection: "ni" → "n<[i]>"
        let spans = vec![
            CharSpan {
                bbox: BBox {
                    x0: 10.0,
                    y0: 10.0,
                    x1: 15.0,
                    y1: 20.0,
                },
                text: "n".to_string(),
                rotation: 0.0,
                font_name: "Arial".to_string(),
                font_size: 12.0,
                font_weight: None,
                char_start_idx: 0,
                char_end_idx: 0,
                original_unicode: Some('n'),
                has_corruption: false,
            },
            CharSpan {
                bbox: BBox {
                    x0: 15.0,
                    y0: 13.0,
                    x1: 18.0,
                    y1: 18.0,
                }, // Lower position (subscript)
                text: "i".to_string(),
                rotation: 0.0,
                font_name: "Arial".to_string(),
                font_size: 9.0, // Smaller font
                font_weight: None,
                char_start_idx: 1,
                char_end_idx: 1,
                original_unicode: Some('i'),
                has_corruption: false,
            },
        ];

        let result = detect_script_notation(&spans);
        assert_eq!(result, "n<[i]>");
    }

    #[test]
    fn test_detect_inline_subscript() {
        // Test common inline subscript patterns
        assert_eq!(detect_inline_subscript("xi"), Some(("x".to_string(), "i".to_string())));
        assert_eq!(detect_inline_subscript("n0"), Some(("n".to_string(), "0".to_string())));
        assert_eq!(detect_inline_subscript("tLT"), Some(("t".to_string(), "LT".to_string())));
        assert_eq!(detect_inline_subscript("ei,j"), Some(("e".to_string(), "i,j".to_string())));
        
        // Should not match regular words
        assert_eq!(detect_inline_subscript("normal"), None);
        assert_eq!(detect_inline_subscript("text"), None);
    }

    #[test]
    fn test_no_script_detection() {
        // Test normal text without subscripts/superscripts
        let spans = vec![
            CharSpan {
                bbox: BBox {
                    x0: 10.0,
                    y0: 10.0,
                    x1: 15.0,
                    y1: 20.0,
                },
                text: "a".to_string(),
                rotation: 0.0,
                font_name: "Arial".to_string(),
                font_size: 12.0,
                font_weight: None,
                char_start_idx: 0,
                char_end_idx: 0,
                original_unicode: None,
                has_corruption: false,
            },
            CharSpan {
                bbox: BBox {
                    x0: 15.0,
                    y0: 10.0,
                    x1: 20.0,
                    y1: 20.0,
                }, // Same vertical position
                text: "b".to_string(),
                rotation: 0.0,
                font_name: "Arial".to_string(),
                font_size: 12.0, // Same font size
                font_weight: None,
                char_start_idx: 1,
                char_end_idx: 1,
                original_unicode: None,
                has_corruption: false,
            },
        ];

        let result = detect_script_notation(&spans);
        assert_eq!(result, "ab"); // No script notation applied
    }
}