//! Text Modification Module
//!
//! This module provides text modification functionality for enhanced readability
//! and improved text processing. It handles mathematical notation, subscripts,
//! superscripts, and other text enhancements that improve accessibility and
//! usability across various applications.
//!
//! ## Usage
//!
//! ### Text Processing with Tags
//! ```text
//! // Process text spans with subscript/superscript detection and HTML tags
//! let enhanced = modtext::unified_text_processing(&char_spans, false);
//! ```
//!
//! ### Feature Flag
//! This module is gated behind the `modtext` feature flag. When disabled,
//! all functions return unmodified text for minimal impact on performance.

use crate::debug_print;
use crate::entities::SpanType;

/// Remove end-of-line hyphens from span text
///
/// This function handles hyphenated words that span across line breaks by:
/// 1. Detecting spans that end with "-" (hyphen)
/// 2. Checking if the next span starts with a letter
/// 3. Removing the hyphen to properly rejoin the word
///
/// Example: "Vi-" + "jil" → "Vijil"
/// Remove line-ending hyphens at the span level
///
/// This function identifies spans that end with "word-" and the next span starts with "word"
/// and combines them into a single "wordword" span, which is the proper way to handle
/// line-ending hyphenation artifacts from PDF text extraction.
fn remove_line_ending_hyphens(spans: &mut Vec<crate::entities::CharSpan>) {
    debug_print!("🔍 HYPHEN REMOVAL: Processing {} spans", spans.len());

    let mut i = 0;
    while i < spans.len().saturating_sub(1) {
        let current_span = &spans[i];
        let next_span = &spans[i + 1];

        // Debug log spans that contain "Vi" or end with hyphens
        if current_span.text.contains("Vi") || current_span.text.ends_with('-') {
            debug_print!(
                "🔍 SPAN {}: '{}' (next: '{}')",
                i,
                current_span.text,
                next_span.text
            );
        }

        // Check if current span ends with line-ending hyphen pattern
        // Only process regular hyphens (U+002D), not em-dashes (U+2014) or en-dashes (U+2013)
        // Note: U+0002 control characters are converted to hyphens in native.rs for consistent handling
        if current_span.text.len() > 1
            && current_span.text.ends_with('-')  // Regular hyphen only
            && !current_span.text.ends_with('—') // Not em-dash
            && !current_span.text.ends_with('–') // Not en-dash
            && current_span.text.chars().rev().nth(1).map(|c| c.is_alphabetic()).unwrap_or(false)
        {
            debug_print!(
                "🔍 FOUND HYPHEN CANDIDATE: '{}' + '{}'",
                current_span.text,
                next_span.text
            );

            // Check if next span starts with word characters
            if !next_span.text.is_empty()
                && next_span
                    .text
                    .chars()
                    .next()
                    .map(|c| c.is_alphabetic())
                    .unwrap_or(false)
            {
                // Skip hyphen removal for likely compound words
                // Common compound word patterns that should be preserved
                // Extract just the last word before the hyphen (not the entire span)
                let text_without_hyphen = current_span.text.trim_end_matches('-');
                let word_before = text_without_hyphen
                    .rsplit(|c: char| c.is_whitespace() || c == '(' || c == ')')
                    .next()
                    .unwrap_or(text_without_hyphen);
                // Extract just the first word from next span (stop at punctuation or space)
                let word_after = next_span
                    .text
                    .split(|c: char| {
                        c.is_whitespace() || c == '.' || c == ',' || c == ')' || c == '('
                    })
                    .next()
                    .unwrap_or(&next_span.text);

                // Check for common compound word patterns
                let likely_compound = is_likely_compound_word(word_before, word_after);

                if likely_compound {
                    debug_print!(
                        "🔍 COMPOUND WORD: Preserving hyphen in '{}-{}' (likely compound word)",
                        word_before,
                        word_after
                    );
                    i += 1;
                    continue;
                }

                // Combine: remove hyphen and join with next span
                // Use the full text without hyphen, not just the word_before
                let combined_text = format!("{}{}", text_without_hyphen, next_span.text);

                debug_print!(
                    "🔗 HYPHEN REMOVAL: '{}' + '{}' → '{}'",
                    current_span.text,
                    next_span.text,
                    combined_text
                );

                // Update current span with combined text
                spans[i].text = combined_text;

                // Remove the next span since it's now combined
                spans.remove(i + 1);

                // Don't increment i since we removed a span
                continue;
            } else {
                debug_print!(
                    "🔍 HYPHEN SKIPPED: Next span doesn't start with word char: '{}'",
                    next_span.text
                );
            }
        }

        i += 1;
    }
}

/// Apply text corrections to individual spans
///
/// This function applies font and text corrections to each span's text content
/// before HTML processing occurs. This ensures that script notation processing
/// works with corrected text rather than original corrupted text.
fn apply_text_corrections_to_spans(spans: &mut [crate::entities::CharSpan]) {
    debug_print!("🔧 SPAN CORRECTIONS: Processing {} spans", spans.len());

    for span in spans.iter_mut() {
        let original_text = span.text.clone();
        let corrected_text = crate::font_analysis::correct_assembled_text(&original_text);

        if corrected_text != original_text {
            debug_print!(
                "🔧 SPAN CORRECTED: '{}' → '{}'",
                original_text,
                corrected_text
            );
            span.text = corrected_text;
        }
    }

    debug_print!("🔧 SPAN CORRECTIONS: Completed span text corrections");
}

#[cfg(feature = "modtext")]
pub mod script_notation;

/// Process text spans to add all appropriate tags (bold, subscript, superscript, formula)
///
/// This is the new unified entry point for all text processing with composable tags.
/// When the `modtext` feature is disabled, this returns the original text unchanged.
///
/// # Example
/// ```text
/// let enhanced = modtext::unified_text_processing(&char_spans, false);
/// ```
/// Unified text processing pipeline for both formulas and text blocks
/// Consolidates hyphen removal, font correction, and HTML tag generation
pub fn unified_text_processing(spans: &[crate::entities::CharSpan], is_formula: bool) -> String {
    debug_print!(
        "🔄 UNIFIED PROCESSING: Processing {} spans, is_formula={}",
        spans.len(),
        is_formula
    );

    // Use shared span processing with simple line break detection
    let mut processed_spans = process_spans_with_line_breaks(&[spans.to_vec()], false);

    // Apply hyphen removal at span level BEFORE HTML processing
    // This fixes line-ending hyphens like "Vi-" + "jil" → "Vijil" at the source
    remove_line_ending_hyphens(&mut processed_spans);

    // Apply text corrections to individual spans BEFORE HTML processing
    // This ensures that the script processing works with corrected text
    apply_text_corrections_to_spans(&mut processed_spans);

    // Apply subscript/superscript detection and HTML tag creation
    let result = script_notation::apply_text_formatting(&processed_spans);

    // Post-process to remove spurious spaces within words
    // This fixes patterns like "ye t" → "yet" where PDF extraction incorrectly split words
    remove_spurious_spaces(&result)
}

/// Remove spurious spaces within words using dictionary validation
///
/// PDF extraction sometimes introduces spaces within words (e.g., "ye t" instead of "yet").
/// This function scans for such patterns and removes the space if joining creates a valid word.
/// It also handles multi-fragment cases like "convers a tions" → "conversations".
#[cfg(feature = "correction-engine")]
fn remove_spurious_spaces(text: &str) -> String {
    use crate::font_analysis::dictionary::SmartCorrector;

    let mut result = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Look for single-space patterns that might be spurious
        if chars[i] == ' ' {
            // Find the word fragment before the space
            let word_start = result
                .rfind(|c: char| c.is_whitespace() || c == ',' || c == '.' || c == ';' || c == ':')
                .map(|idx| idx + 1)
                .unwrap_or(0);
            let word_before = &result[word_start..];

            // Find the word fragment after the space (until next space/punctuation)
            let mut word_after_end = i + 1;
            while word_after_end < chars.len()
                && !chars[word_after_end].is_whitespace()
                && !matches!(chars[word_after_end], ',' | '.' | ';' | ':')
            {
                word_after_end += 1;
            }
            let word_after: String = chars[i + 1..word_after_end].iter().collect();

            // Never join across standalone dashes — they are semantic separators
            // (e.g., "input - detecting" should NOT become "input-detecting")
            let is_dash_like =
                word_after == "-" || word_after == "--" || word_after == "—" || word_after == "–";
            let before_ends_with_dash = word_before.ends_with('-')
                || word_before.ends_with('—')
                || word_before.ends_with('–');

            if is_dash_like || before_ends_with_dash {
                result.push(chars[i]);
                i += 1;
                continue;
            }

            // Check if joining would create a valid word from fragments
            if crate::spacing::should_join_fragments(word_before, &word_after) {
                // Don't add the space - continue without it
                i += 1;
                continue;
            }

            // Multi-fragment lookahead: check if there's another short fragment after word_after
            // This handles cases like "convers a tions" where "conversa" and "ations" aren't words
            // but "conversations" is valid
            if !word_before.is_empty()
                && !word_after.is_empty()
                && word_after.len() <= 3
                && word_after_end < chars.len()
                && chars[word_after_end] == ' '
            {
                // Find the next fragment after the second space
                let mut next_frag_end = word_after_end + 1;
                while next_frag_end < chars.len()
                    && !chars[next_frag_end].is_whitespace()
                    && !matches!(chars[next_frag_end], ',' | '.' | ';' | ':')
                {
                    next_frag_end += 1;
                }
                let next_fragment: String =
                    chars[word_after_end + 1..next_frag_end].iter().collect();

                // Try joining all three fragments
                let triple_len = word_before.len() + word_after.len() + next_fragment.len();
                if !next_fragment.is_empty() && (2..=20).contains(&triple_len) {
                    let triple_joined = format!("{}{}{}", word_before, word_after, next_fragment);

                    if SmartCorrector::is_valid_word(&triple_joined) {
                        // Skip this space AND the fragment AND the next space
                        // We'll handle them all at once by not adding the space
                        // and letting the next iterations handle the rest
                        i += 1;
                        continue;
                    }
                }
            }
        }

        result.push(chars[i]);
        i += 1;
    }

    result
}

#[cfg(not(feature = "correction-engine"))]
fn remove_spurious_spaces(text: &str) -> String {
    text.to_string()
}

/// Legacy wrapper for add_tags - now uses unified processing
/// This maintains backward compatibility while using the new unified pipeline
pub fn add_tags(spans: &[crate::entities::CharSpan]) -> String {
    unified_text_processing(spans, true) // Formulas get HTML corrections
}

/// Process CharSpans into a flattened vector with line break detection
///
/// This shared function handles the common logic for both formula and text processing:
/// - Flattens line_spans into a single vector
/// - Detects y-coordinate jumps indicating line breaks
/// - Inserts semicolon separators for multi-line content
/// - Handles both simple and complex line break detection patterns
fn process_spans_with_line_breaks(
    line_spans: &[Vec<crate::entities::CharSpan>],
    use_complex_detection: bool,
) -> Vec<crate::entities::CharSpan> {
    let mut all_spans = Vec::new();

    let mut prev_y: Option<f32> = None;
    let mut prev_significant_y: Option<f32> = None;
    const LINE_BREAK_THRESHOLD: f32 = 10.0;
    const SMALL_SPAN_THRESHOLD: f32 = 5.0;

    for line in line_spans {
        for span in line {
            // Skip truly empty spans for text processing, but preserve space spans
            if !use_complex_detection && span.text.is_empty() {
                continue;
            }

            let current_y = span.bbox.y0;

            // Add semicolons as line break separators ONLY for formulas (use_complex_detection=true)
            if use_complex_detection {
                let should_insert_semicolon = if let Some(py) = prev_y {
                    let y_diff = current_y - py;
                    if y_diff > LINE_BREAK_THRESHOLD {
                        true
                    } else if let Some(sig_y) = prev_significant_y {
                        let total_diff = current_y - sig_y;
                        y_diff > SMALL_SPAN_THRESHOLD && total_diff > LINE_BREAK_THRESHOLD
                    } else {
                        false
                    }
                } else {
                    false
                };

                if should_insert_semicolon {
                    let separator_span = crate::entities::CharSpan {
                        text: "; ".to_string(),
                        bbox: span.bbox.clone(),
                        rotation: span.rotation,
                        font_name: span.font_name.clone(),
                        font_size: span.font_size,
                        font_weight: span.font_weight,
                        char_start_idx: span.char_start_idx,
                        char_end_idx: span.char_end_idx,
                        original_unicode: None,
                        has_corruption: false,
                        span_type: SpanType::Normal,
                    };
                    all_spans.push(separator_span);

                    let ref_y = prev_significant_y.unwrap_or(prev_y.unwrap());
                    let total_diff = current_y - ref_y;
                    debug_print!("➕ FORMULA LINE BREAK: Added semicolon separator for y-jump {ref_y:.1} -> {current_y:.1} (diff: +{total_diff:.1})");
                }
            }

            all_spans.push(span.clone());

            // Update tracking variables for formula processing
            if use_complex_detection {
                prev_y = Some(current_y);
                // Update significant y-position for non-punctuation spans
                if !span.text.trim().is_empty()
                    && span.text.trim() != "."
                    && span.text.trim() != ","
                {
                    prev_significant_y = Some(current_y);
                }
            }
        }
    }

    all_spans
}

/// Process text with CharSpans for subscript detection but without formula wrapper
/// This is like format_formula_with_spans but returns plain text for TEXT elements
pub fn process_text_with_spans(
    text: &str,
    line_spans: &[Vec<crate::entities::CharSpan>],
) -> String {
    debug_print!(
        "📋 TEXT WITH SPANS: Processing text with {} line(s) of spans: {}",
        line_spans.len(),
        text.chars().take(100).collect::<String>()
    );

    // Flatten spans for unified processing
    let all_spans: Vec<crate::entities::CharSpan> = line_spans
        .iter()
        .flat_map(|line| line.iter())
        .cloned()
        .collect();

    // Use unified processing pipeline for text (not formula)
    let final_text = unified_text_processing(&all_spans, false);

    debug_print!(
        "📋 TEXT WITH SPANS: Unified processing result: '{}'",
        final_text.chars().take(100).collect::<String>()
    );

    // Clean up substitute characters
    let cleaned_text = final_text.replace('\u{001a}', "");

    debug_print!(
        "📋 TEXT WITH SPANS FINAL: '{}'",
        cleaned_text.chars().take(100).collect::<String>()
    );

    cleaned_text
}

/// Determine if two word parts likely form a compound word that should keep its hyphen
///
/// This function identifies common compound word patterns to prevent incorrect hyphen removal.
/// Returns true if the hyphen should be preserved, false if it's likely a line-ending artifact.
fn is_likely_compound_word(word_before: &str, word_after: &str) -> bool {
    let before_lower = word_before.to_lowercase();
    let after_lower = word_after.to_lowercase();

    // Common compound word patterns - not currently used since we rely on dictionary
    // But kept here for reference if needed
    let compound_patterns: [(&str, &str); 0] = [];

    // Check exact pattern matches
    for (first, second) in compound_patterns.iter() {
        if (first.is_empty() || before_lower.ends_with(first))
            && (second.is_empty() || after_lower.starts_with(second))
        {
            return true;
        }
    }

    // Additional heuristics for compound words:

    // Numbers followed by units or descriptors (not usually line breaks)
    if word_before.chars().any(|c| c.is_numeric()) && word_after.len() <= 8 {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Initialize the dictionary for tests that need it
    #[cfg(feature = "correction-engine")]
    fn init_dictionary() {
        use crate::font_analysis::dictionary::{SmartCorrectionConfig, SmartCorrector};
        let config = SmartCorrectionConfig::default();
        let _ = SmartCorrector::new(config);
    }

    #[test]
    #[cfg(feature = "correction-engine")]
    fn test_remove_spurious_spaces_preserves_dash_separators() {
        init_dictionary();
        // Standalone dashes between words should be preserved with their spaces
        let text = "evaluating user input - detecting malicious content";
        let result = remove_spurious_spaces(text);
        assert_eq!(
            result, text,
            "Space before standalone dash should be preserved"
        );
    }

    #[test]
    #[cfg(feature = "correction-engine")]
    fn test_remove_spurious_spaces_preserves_double_dash() {
        init_dictionary();
        let text = "categories -- direct injection attacks";
        let result = remove_spurious_spaces(text);
        assert_eq!(
            result, text,
            "Spaces around double dash should be preserved"
        );
    }

    #[test]
    #[cfg(feature = "correction-engine")]
    fn test_remove_spurious_spaces_preserves_em_dash() {
        init_dictionary();
        let text = "models \u{2014} where alignment";
        let result = remove_spurious_spaces(text);
        assert_eq!(result, text, "Spaces around em-dash should be preserved");
    }

    #[test]
    #[cfg(feature = "correction-engine")]
    fn test_remove_spurious_spaces_preserves_en_dash() {
        init_dictionary();
        let text = "models \u{2013} where alignment";
        let result = remove_spurious_spaces(text);
        assert_eq!(result, text, "Spaces around en-dash should be preserved");
    }

    #[test]
    #[cfg(feature = "correction-engine")]
    fn test_remove_spurious_spaces_still_fixes_split_words() {
        init_dictionary();
        // The normal case should still work: spurious spaces within words
        let text = "ye t another wo rd";
        let result = remove_spurious_spaces(text);
        assert_eq!(
            result, "yet another word",
            "Spurious spaces within words should still be removed"
        );
    }
}
