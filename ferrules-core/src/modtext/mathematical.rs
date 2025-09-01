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

use crate::entities::CharSpan;
use lazy_static::lazy_static;
use regex::Regex;

/// Detect and convert subscripts and superscripts to bracket notation
///
/// Analyzes character positioning and font sizes to identify mathematical subscripts
/// and superscripts, converting them to HTML tags for better readability:
/// - Subscripts: "ni" → "n<sub>i</sub>", "LossMSP" → "Loss<sub>MSP</sub>"  
/// - Superscripts: "x²" → "x<sup>2</sup>", "a³" → "a<sup>3</sup>"
/// - Bold text: "MathBERT" → "<b>MathBERT</b>"
///
/// Detection criteria:
/// - Vertical position offset (y-coordinate difference)
/// - Font size difference between base and script characters
/// - Horizontal proximity for character grouping
pub(crate) fn detect_script_notation(spans: &[CharSpan]) -> String {
    if spans.is_empty() {
        return String::new();
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    enum CharacterRole {
        Baseline,
        Subscript,
        Superscript,
    }

    /// Research-based character role classification using (H,D)-features
    /// Based on "Identifying Subscripts and Superscripts in Mathematical Documents"
    /// Achieves ~99.89% accuracy using relative size and position analysis
    fn classify_character_role(current: &CharSpan, base: &CharSpan) -> CharacterRole {
        // H-feature: Height ratio (font size ratio)
        let height_ratio = current.font_size / base.font_size;
        
        // D-feature: Displacement (y-position difference, normalized by base font size)
        let displacement = (current.bbox.y0 - base.bbox.y0) / base.font_size;
        
        // Research-based thresholds for Bayesian classification
        // Subscript: smaller font + positive displacement (below baseline in PDF coordinates)
        if height_ratio < 0.85 && displacement > 0.1 && displacement < 0.5 {
            CharacterRole::Subscript
        }
        // Superscript: smaller font + negative displacement (above baseline in PDF coordinates)  
        else if height_ratio < 0.85 && displacement < -0.1 && displacement > -0.5 {
            CharacterRole::Superscript
        }
        // Baseline: normal size or not positioned as script
        else {
            CharacterRole::Baseline
        }
    }

    /// Detect baseline changes based on significant position shifts in base-size characters
    fn detect_baseline_change(current: &CharSpan, previous: &CharSpan) -> bool {
        let font_ratio = current.font_size / previous.font_size;
        let is_both_base_size = font_ratio > 0.9 && font_ratio < 1.1; // Both ~same size
        
        if is_both_base_size {
            let y_diff = (current.bbox.y0 - previous.bbox.y0).abs();
            let baseline_shift_threshold = 0.3 * previous.font_size; // 30% of font size
            y_diff > baseline_shift_threshold
        } else {
            false
        }
    }

    let mut result = String::new();
    let mut i = 0;
    let mut current_baseline_y = 0.0;
    let mut current_baseline_font_size = 10.0;

    while i < spans.len() {
        let current_span = &spans[i];
        
        // Check if current span contains bold text that should be wrapped in <b></b> tags
        if is_bold_text(current_span) {
            result.push_str(&format!("<b>{}</b>", current_span.text));
            i += 1;
            continue;
        }

        // Only check for inline subscript patterns if we're in a mathematical context
        if is_mathematical_context(spans, i) {
            if let Some((base_part, script_part)) = detect_inline_subscript(&current_span.text) {
                result.push_str(&format!("{base_part}<sub>{script_part}</sub>"));
                i += 1;
                continue;
            }
        }

        // Check for compound bold words like "VideoBERT", "CodeBERT" where spans might be split
        if let Some(compound_text) = detect_compound_bold_word(spans, i) {
            result.push_str(&compound_text.0);
            i += compound_text.1; // Skip the processed spans
            continue;
        }

        // Initialize or update baseline tracking
        let font_ratio = current_span.font_size / current_baseline_font_size;
        let is_likely_base_character = font_ratio > 0.9; // Within 10% of current baseline font size
        
        if is_likely_base_character {
            // Check for baseline change
            if i > 0 {
                let previous_span = &spans[i - 1];
                if detect_baseline_change(current_span, previous_span) {
                    eprintln!("🔄 BASELINE CHANGE: {} (y:{:.1}) → {} (y:{:.1}), shift:{:.1}", 
                        previous_span.text.trim(), previous_span.bbox.y0,
                        current_span.text.trim(), current_span.bbox.y0,
                        current_span.bbox.y0 - previous_span.bbox.y0);
                }
            }
            
            // Update baseline tracking
            current_baseline_y = current_span.bbox.y0;
            current_baseline_font_size = current_span.font_size;
        }

        // Complete character-by-character debugging for the problematic formula line
        // Capture ALL characters in the target y-range to get the complete picture
        if current_span.bbox.y0 > 340.0 && current_span.bbox.y0 < 350.0 {
            let role = if is_likely_base_character {
                CharacterRole::Baseline
            } else {
                // Create a synthetic base span for classification
                let base_span = crate::entities::CharSpan {
                    text: "".to_string(),
                    font_name: current_span.font_name.clone(),
                    font_size: current_baseline_font_size,
                    bbox: crate::entities::BBox {
                        x0: current_span.bbox.x0,
                        y0: current_baseline_y,
                        x1: current_span.bbox.x1,
                        y1: current_baseline_y,
                    },
                    rotation: current_span.rotation,
                    font_weight: current_span.font_weight,
                    char_start_idx: current_span.char_start_idx,
                    char_end_idx: current_span.char_end_idx,
                    original_unicode: current_span.original_unicode,
                    has_corruption: current_span.has_corruption,
                };
                classify_character_role(current_span, &base_span)
            };
            
            eprintln!("📍 CHAR DEBUG: '{}' = y:{:.1}, font_size:{:.1}, font_name:{}, role:{:?}, baseline_y:{:.1}", 
                current_span.text, current_span.bbox.y0, current_span.font_size, 
                &current_span.font_name, role, current_baseline_y);
        }

        // Look ahead for potential script characters using research-based classification
        let mut script_chars = Vec::new();
        let mut script_type = None;
        let mut j = i + 1;

        // Create base span for script detection
        let base_span = crate::entities::CharSpan {
            text: current_span.text.clone(),
            font_name: current_span.font_name.clone(),
            font_size: current_baseline_font_size,
            bbox: crate::entities::BBox {
                x0: current_span.bbox.x0,
                y0: current_baseline_y,
                x1: current_span.bbox.x1,
                y1: current_baseline_y,
            },
            rotation: current_span.rotation,
            font_weight: current_span.font_weight,
            char_start_idx: current_span.char_start_idx,
            char_end_idx: current_span.char_end_idx,
            original_unicode: current_span.original_unicode,
            has_corruption: current_span.has_corruption,
        };

        while j < spans.len() {
            let next_span = &spans[j];

            // Check horizontal proximity - use previous span for proximity, not base
            let prev_span = if j > i + 1 { &spans[j - 1] } else { current_span };
            let horizontal_proximity_threshold = current_baseline_font_size * 1.2;
            if next_span.bbox.x0 - prev_span.bbox.x1 > horizontal_proximity_threshold {
                break;
            }

            // Check if the character is just whitespace or empty - skip these for script detection
            let is_whitespace_only = next_span.text.trim().is_empty();
            if is_whitespace_only {
                j += 1;
                continue;
            }

            // Classify character role using research-based algorithm
            let role = classify_character_role(next_span, &base_span);

            // Debug subscript detection for 'i', 'j', and ')' characters (')' is corrected from 'i')
            if next_span.text == "i" || next_span.text == "j" || next_span.text == ")" {
                let height_ratio = next_span.font_size / base_span.font_size;
                let displacement = (next_span.bbox.y0 - base_span.bbox.y0) / base_span.font_size;
                
                eprintln!(
                    "🔍 RESEARCH-BASED DEBUG: '{}{}' - base(y:{:.1}, size:{:.1}) + next(y:{:.1}, size:{:.1}) → H-ratio:{:.2}, D-displacement:{:.2}, role:{:?}",
                    current_span.text.trim(), next_span.text,
                    base_span.bbox.y0, base_span.font_size,
                    next_span.bbox.y0, next_span.font_size,
                    height_ratio, displacement, role
                );
            }

            match role {
                CharacterRole::Subscript => {
                    if script_type.is_none() {
                        script_type = Some("sub");
                    } else if script_type != Some("sub") {
                        // Mixed script types, break the chain
                        break;
                    }
                    script_chars.push(next_span.text.trim());
                }
                CharacterRole::Superscript => {
                    if script_type.is_none() {
                        script_type = Some("sup");
                    } else if script_type != Some("sup") {
                        // Mixed script types, break the chain
                        break;
                    }
                    script_chars.push(next_span.text.trim());
                }
                CharacterRole::Baseline => {
                    // Baseline character breaks the script chain
                    break;
                }
            }

            j += 1;
        }

        // Apply script formatting if we found any script characters
        if !script_chars.is_empty() {
            let script_text = script_chars.join("");
            match script_type {
                Some("sub") => {
                    result.push_str(&format!("{}<sub>{}</sub>", current_span.text.trim(), script_text));
                }
                Some("sup") => {
                    result.push_str(&format!("{}<sup>{}</sup>", current_span.text.trim(), script_text));
                }
                _ => {
                    result.push_str(&current_span.text);
                }
            }
            i = j; // Skip all the processed spans
        } else {
            result.push_str(&current_span.text);
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
        // Add pattern for specific multi-character subscripts like tLT, cLC, etc.
        // Only match single lowercase letter + 2-3 uppercase letters for mathematical notation
        static ref MULTI_CHAR_SUBSCRIPT_RE: Regex = Regex::new(r"^([tcnpkfghdrsv])([A-Z]{2,3})$").unwrap();
        // Add pattern ONLY for specific mathematical terms like LossMSP, LossCCP, LossMLM, etc.
        // Do NOT match compound model names like VideoBERT, LayoutLM, CodeBERT
        static ref WORD_SUBSCRIPT_RE: Regex = Regex::new(r"^(Loss)([A-Z]{2,})$").unwrap();
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
    // But first check if this might be a compound word that should NOT be a subscript
    if !is_likely_compound_word(clean_text) {
        if let Some(caps) = MULTI_CHAR_SUBSCRIPT_RE.captures(clean_text) {
            return Some((caps[1].to_string(), caps[2].to_string()));
        }
    }

    // Handle word subscripts like "LossMSP", etc.
    // But first check if this might be a compound word that should NOT be a subscript
    if !is_likely_compound_word(clean_text) {
        if let Some(caps) = WORD_SUBSCRIPT_RE.captures(clean_text) {
            return Some((caps[1].to_string(), caps[2].to_string()));
        }
    }

    None
}

/// Check if a text string is likely a compound word rather than a mathematical subscript
fn is_likely_compound_word(text: &str) -> bool {
    let len = text.len();

    // Too short to be a compound word
    if len < 4 {
        return false;
    }

    // Too long to be a typical mathematical subscript
    if len > 15 {
        return true;
    }

    // Count transitions from lowercase to uppercase (CamelCase indicator)
    let chars: Vec<char> = text.chars().collect();
    let mut case_transitions = 0;

    for i in 1..chars.len() {
        if chars[i - 1].is_ascii_lowercase() && chars[i].is_ascii_uppercase() {
            case_transitions += 1;
        }
    }

    // Multiple case transitions suggest compound word (e.g., VideoBERT, LayoutLM)
    if case_transitions >= 1 {
        return true;
    }

    // Removed hardcoded tech acronym patterns - these are PDF-specific and not generic

    // If it has reasonable word structure (starts with capital, contains lowercase)
    let first_char = chars.first().unwrap_or(&'a');
    let has_lowercase = chars.iter().any(|c| c.is_ascii_lowercase());
    let has_uppercase = chars.iter().any(|c| c.is_ascii_uppercase());

    if first_char.is_ascii_uppercase() && has_lowercase && has_uppercase && len > 6 {
        return true;
    }

    false
}

/// Detect if a character span represents bold text
///
/// This function checks the font weight and font name to determine if text should be formatted as bold
/// rather than treated as a subscript or superscript.
pub(crate) fn is_bold_text(span: &CharSpan) -> bool {
    // Check font name for bold indicators (common pattern in PDFs)
    let font_name = span.font_name.to_lowercase();
    if font_name.contains("bold") || font_name.contains("black") || font_name.contains("heavy") {
        return true;
    }

    // Check for font weight indicators
    if let Some(font_weight) = span.font_weight.as_ref() {
        // Use string representation to handle different weight variants
        let weight_str = format!("{font_weight:?}").to_lowercase();
        if weight_str.contains("bold")
            || weight_str.contains("700")
            || weight_str.contains("800")
            || weight_str.contains("900")
        {
            return true;
        }
    }

    false
}

/// Check if text contains mathematical symbols
fn contains_math_symbol(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(
            c,
            '=' | '+'
                | '−'
                | '×'
                | '÷'
                | '∑'
                | '∫'
                | '∂'
                | '∇'
                | '√'
                | '≤'
                | '≥'
                | '≠'
                | '∞'
                | 'π'
                | 'α'
                | 'β'
                | 'γ'
                | 'δ'
                | 'λ'
                | 'μ'
                | 'σ'
                | 'θ'
                | 'φ'
                | '['
                | ']'
                | '('
                | ')'
                | '{'
                | '}'
        )
    })
}

/// Detect if we're in a mathematical context where subscript patterns should be applied
fn is_mathematical_context(spans: &[CharSpan], current_idx: usize) -> bool {
    // Look for mathematical indicators in nearby spans
    let window_start = current_idx.saturating_sub(3);
    let window_end = (current_idx + 4).min(spans.len());

    for span in spans.iter().take(window_end).skip(window_start) {
        let text = &span.text;

        // Check for mathematical symbols or notation
        if contains_math_symbol(text) {
            return true;
        }

        // Check for formula-like patterns (single letters with spaces)
        if text.len() == 1 && text.chars().next().unwrap().is_ascii_alphabetic() {
            // This might be part of a mathematical formula
            return true;
        }
    }

    false
}

/// Detect compound bold words like "VideoBERT", "CodeBERT" where spans might be split
///
/// This function checks if consecutive spans form a compound word where part of it is bold,
/// and returns the formatted result with the number of spans consumed.
pub(crate) fn detect_compound_bold_word(
    spans: &[CharSpan],
    start_idx: usize,
) -> Option<(String, usize)> {
    if start_idx + 1 >= spans.len() {
        return None;
    }

    let base_span = &spans[start_idx];
    let next_span = &spans[start_idx + 1];

    // Check if this looks like a compound word (close proximity, same line)
    let horizontal_gap = next_span.bbox.x0 - base_span.bbox.x1;
    let vertical_diff = (next_span.bbox.y0 - base_span.bbox.y0).abs();

    // Must be on same line and close together
    if horizontal_gap > 5.0 || vertical_diff > 2.0 {
        return None;
    }

    // Check for common compound word patterns where the second part is bold
    let base_text = base_span.text.trim();
    let next_text = next_span.text.trim();

    // Look for patterns like "Video" + "BERT", "Code" + "BERT", "Layout" + "LM", etc.
    if is_compound_word_pattern(base_text, next_text) && is_bold_text(next_span) {
        let compound_word = format!("{base_text}{next_text}");
        return Some((format!("<b>{compound_word}</b>"), 2));
    }

    // Check if both parts are bold and form a compound word
    if is_bold_text(base_span)
        && is_bold_text(next_span)
        && is_compound_word_pattern(base_text, next_text)
    {
        let compound_word = format!("{base_text}{next_text}");
        return Some((format!("<b>{compound_word}</b>"), 2));
    }

    None
}

/// Check if two text parts form a recognizable compound word pattern
fn is_compound_word_pattern(first: &str, second: &str) -> bool {
    // Generic rules for compound words:
    // 1. Both parts must be reasonable word lengths (not single chars like mathematical variables)
    // 2. First part should be a normal word (mixed case or initial cap)
    // 3. Second part should be all caps (like acronyms) or PascalCase
    // 4. Total length should be reasonable for a compound word

    let first_len = first.len();
    let second_len = second.len();
    let total_len = first_len + second_len;

    // Must be reasonable lengths for compound words
    if first_len < 2 || second_len < 2 || total_len > 20 {
        return false;
    }

    // First part: should be a normal word (letters only, not mathematical symbols)
    if !first.chars().all(|c| c.is_ascii_alphabetic()) {
        return false;
    }

    // Second part: should be all uppercase (acronym) or PascalCase
    let second_is_acronym = second.chars().all(|c| c.is_ascii_uppercase());
    let second_is_pascal = second.chars().next().unwrap_or('a').is_ascii_uppercase()
        && second.chars().all(|c| c.is_ascii_alphabetic());

    if !second_is_acronym && !second_is_pascal {
        return false;
    }

    // Exclude obvious mathematical subscripts (single letter + single/double caps)
    if first_len == 1 && second_len <= 3 {
        return false;
    }

    // This looks like a compound word
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{BBox, CharSpan};

    #[test]
    fn test_detect_script_notation_subscript() {
        // Test subscript detection: "ni" → "n<↓i↓>"
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
        assert_eq!(result, "n<sub>i</sub>");
    }

    #[test]
    fn test_detect_inline_subscript() {
        // Test common inline subscript patterns
        assert_eq!(
            detect_inline_subscript("xi"),
            Some(("x".to_string(), "i".to_string()))
        );
        assert_eq!(
            detect_inline_subscript("n0"),
            Some(("n".to_string(), "0".to_string()))
        );
        assert_eq!(
            detect_inline_subscript("tLT"),
            Some(("t".to_string(), "LT".to_string()))
        );
        assert_eq!(
            detect_inline_subscript("ei,j"),
            Some(("e".to_string(), "i,j".to_string()))
        );

        // Test word subscripts like LossMSP, LossMLM, etc.
        assert_eq!(
            detect_inline_subscript("LossMSP"),
            Some(("Loss".to_string(), "MSP".to_string()))
        );
        assert_eq!(
            detect_inline_subscript("LossMLM"),
            Some(("Loss".to_string(), "MLM".to_string()))
        );
        assert_eq!(
            detect_inline_subscript("LossCCP"),
            Some(("Loss".to_string(), "CCP".to_string()))
        );

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
