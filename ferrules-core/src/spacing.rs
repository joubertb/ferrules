/// Font-aware text spacing utilities
///
/// This module provides consistent spacing logic across all text processing pipelines
/// to ensure proper word boundaries are detected in PDF text extraction.
use crate::entities::CharSpan;

#[cfg(feature = "correction-engine")]
use crate::font_analysis::dictionary::SmartCorrector;

/// Character width estimation factor
/// This represents the average character width as a fraction of font size
/// Based on typical proportional font characteristics
const AVERAGE_CHAR_WIDTH_FACTOR: f32 = 0.55;

/// Font-aware spacing threshold factor
/// This determines the minimum horizontal gap (as fraction of character width)
/// needed to consider adding a space between spans
///
/// 0.16 = 1/6 of average character width
/// - Works well for small gaps (0.9pt) between different fonts
/// - Sensitive enough to catch word boundaries with minimal spacing
/// - Conservative enough to avoid false positives
const WORD_BOUNDARY_THRESHOLD_FACTOR: f32 = 0.16;

/// Maximum gap (as multiple of threshold) for word validation
/// When the gap is small enough (within this multiple of the threshold),
/// we check if joining the spans creates a valid word before adding a space.
/// This prevents spurious spaces in words like "ye t" → "yet"
const WORD_VALIDATION_GAP_MULTIPLE: f32 = 3.0;

/// Maximum gap (as multiple of base threshold) for digit-digit adjacency.
/// When both spans are digits on the same line, we require a larger gap before
/// inserting a space. This prevents "100" from becoming "1 00" when the PDF
/// typesets digits with slightly wider kerning.
/// 3.0x threshold ≈ 2.6pt for 10pt font, vs real word spaces at ~2.5pt+
const DIGIT_ADJACENCY_GAP_MULTIPLE: f32 = 3.0;

/// Calculate font-aware spacing threshold for a given font size
///
/// This function determines the minimum horizontal gap needed between character spans
/// to trigger space insertion. The threshold is calculated as a fraction of the
/// estimated average character width for the given font size.
///
/// # Arguments
/// * `font_size` - The font size in points
///
/// # Returns
/// The minimum horizontal gap (in points) needed to trigger space insertion
///
/// # Example
/// For 9pt font: threshold = 9 * 0.55 * 0.16 ≈ 0.79 points
/// For 16pt font: threshold = 16 * 0.55 * 0.16 ≈ 1.41 points
pub fn calculate_spacing_threshold(font_size: f32) -> f32 {
    let estimated_avg_char_width = font_size * AVERAGE_CHAR_WIDTH_FACTOR;
    estimated_avg_char_width * WORD_BOUNDARY_THRESHOLD_FACTOR
}

/// Determine if a space should be added between two character spans
///
/// This function implements the unified spacing logic used across all text processing
/// pipelines. It considers:
/// - Font-aware horizontal gap thresholds
/// - Vertical differences (line wrapping)
/// - Existing spaces at span boundaries
/// - Line wrapping patterns
///
/// # Arguments
/// * `prev_span` - The previous character span
/// * `curr_span` - The current character span
/// * `line_wrap_threshold` - Threshold for vertical differences indicating line wraps (typically 5.0)
///
/// # Returns
/// `true` if a space should be inserted between the spans
pub fn should_add_space_between_spans(
    prev_span: &CharSpan,
    curr_span: &CharSpan,
    line_wrap_threshold: f32,
) -> bool {
    // Calculate horizontal and vertical gaps between the spans
    let horizontal_gap = curr_span.bbox.x0 - prev_span.bbox.x1;
    let vertical_difference = (curr_span.bbox.y0 - prev_span.bbox.y0).abs();

    // Calculate font-aware threshold based on previous span's font size
    let min_word_boundary_gap = calculate_spacing_threshold(prev_span.font_size);

    // Check if either span already has spacing at boundaries
    let prev_text_ends_with_space = prev_span.text.ends_with(' ');
    let curr_text_starts_with_space = curr_span.text.starts_with(' ');
    let spans_already_have_spacing = prev_text_ends_with_space || curr_text_starts_with_space;

    // Define line wrapping detection constants
    const NEGATIVE_GAP_LINE_WRAP_THRESHOLD: f32 = -10.0; // Large negative gap indicates line wrap
    const SMALL_VERTICAL_DIFF_THRESHOLD: f32 = 3.0; // Small Y difference for wrapped text

    // Determine if spacing is needed based on various conditions
    let horizontal_gap_indicates_word_boundary = horizontal_gap > min_word_boundary_gap;
    let vertical_gap_indicates_line_wrap = vertical_difference > line_wrap_threshold;
    let negative_gap_with_small_y_diff_suggests_line_wrap = horizontal_gap
        < NEGATIVE_GAP_LINE_WRAP_THRESHOLD
        && vertical_difference < SMALL_VERTICAL_DIFF_THRESHOLD;

    let spacing_conditions_met = horizontal_gap_indicates_word_boundary
        || vertical_gap_indicates_line_wrap
        || negative_gap_with_small_y_diff_suggests_line_wrap;

    // Only add space if conditions are met AND spans don't already have spacing
    if !spacing_conditions_met || spans_already_have_spacing {
        return false;
    }

    // Don't insert space inside decimal numbers (digit.digit).
    // PDFs often typeset math expressions with extra spacing around the decimal point,
    // causing the gap to exceed the word-boundary threshold. Detect and suppress.
    //
    // Case 1: prev="0." curr="1" — prev ends with digit-dot, curr starts with digit
    // Case 2: prev="." curr="1" — standalone period, curr starts with digit
    // Case 3: prev="= 0" curr="." — prev ends with digit, curr is standalone period
    if curr_span.text.starts_with(|c: char| c.is_ascii_digit()) {
        let prev_bytes = prev_span.text.as_bytes();
        let prev_len = prev_bytes.len();
        if prev_len >= 2
            && prev_bytes[prev_len - 1] == b'.'
            && prev_bytes[prev_len - 2].is_ascii_digit()
        {
            return false;
        }
        if prev_span.text == "." {
            return false;
        }
    }
    // Case 3: curr is a standalone "." and prev ends with a digit
    if curr_span.text == "." && prev_span.text.ends_with(|c: char| c.is_ascii_digit()) {
        return false;
    }

    // Don't insert space between adjacent digits on the same line.
    // PDFs sometimes typeset numbers with wider kerning (e.g., "100" becomes spans "1" + "00").
    // Require a larger gap (DIGIT_ADJACENCY_GAP_MULTIPLE × threshold) before splitting digits.
    // Use the larger font size when spans differ — PDFs may render the leading digit smaller
    // (e.g., "1" at 4pt + "00%" at 10pt for "100%"), and using the small font's threshold
    // would incorrectly allow a space.
    if horizontal_gap_indicates_word_boundary
        && !vertical_gap_indicates_line_wrap
        && !negative_gap_with_small_y_diff_suggests_line_wrap
    {
        let prev_ends_digit = prev_span.text.ends_with(|c: char| c.is_ascii_digit());
        let curr_starts_digit = curr_span.text.starts_with(|c: char| c.is_ascii_digit());
        if prev_ends_digit && curr_starts_digit {
            // Use the larger font size when spans have different sizes.
            let mut max_font_size = prev_span.font_size.max(curr_span.font_size);
            // For macOS Quartz PDFs where all font_sizes are 1.0, derive effective
            // font size from the larger span's bbox height instead.
            if max_font_size <= 1.0 {
                let prev_height = (prev_span.bbox.y1 - prev_span.bbox.y0).abs();
                let curr_height = (curr_span.bbox.y1 - curr_span.bbox.y0).abs();
                let max_height = prev_height.max(curr_height);
                if max_height > 2.0 {
                    max_font_size = max_height;
                }
            }
            let digit_threshold = calculate_spacing_threshold(max_font_size);
            let digit_gap_limit = digit_threshold * DIGIT_ADJACENCY_GAP_MULTIPLE;
            if horizontal_gap < digit_gap_limit {
                return false;
            }
        }
    }

    // Word validation: if the gap is small and joining creates a valid word, don't add space
    // This prevents spurious spaces like "ye t" when it should be "yet"
    #[cfg(feature = "correction-engine")]
    {
        let word_validation_gap_limit = min_word_boundary_gap * WORD_VALIDATION_GAP_MULTIPLE;

        // Compare base font names (strip subset prefix like "BCDEFE+" before comparing)
        let prev_base_font = prev_span
            .font_name
            .split('+')
            .next_back()
            .unwrap_or(&prev_span.font_name);
        let curr_base_font = curr_span
            .font_name
            .split('+')
            .next_back()
            .unwrap_or(&curr_span.font_name);
        let same_font = prev_base_font == curr_base_font;

        // Only apply word validation for small horizontal gaps, same font, not line wraps.
        // Different fonts indicate different semantic entities (e.g., math variable "D" in italic
        // vs body text "is" in roman) — the space between them is intentional.
        if horizontal_gap_indicates_word_boundary
            && !vertical_gap_indicates_line_wrap
            && horizontal_gap < word_validation_gap_limit
            && same_font
        {
            // Get the last word fragment from prev_span and first word fragment from curr_span
            let prev_word_end = prev_span
                .text
                .rsplit(|c: char| c.is_whitespace() || c == ',' || c == '.')
                .next()
                .unwrap_or(&prev_span.text);
            let curr_word_start = curr_span
                .text
                .split(|c: char| c.is_whitespace() || c == ',' || c == '.')
                .next()
                .unwrap_or(&curr_span.text);

            // Never suppress spaces around standalone dashes — they are semantic separators
            let is_dash_boundary = prev_word_end.ends_with('-')
                || prev_word_end.ends_with('—')
                || prev_word_end.ends_with('–')
                || curr_word_start.starts_with('-')
                || curr_word_start.starts_with('—')
                || curr_word_start.starts_with('–');

            // Only check if both fragments are non-empty and the combined length is reasonable
            if !is_dash_boundary
                && !prev_word_end.is_empty()
                && !curr_word_start.is_empty()
                && prev_word_end.len() + curr_word_start.len() <= 15
            {
                let joined = format!("{}{}", prev_word_end, curr_word_start);

                // If joined text is a valid English word, don't add space
                if SmartCorrector::is_valid_word(&joined) {
                    return false;
                }
            }
        }
    }

    true
}

/// Determine if a space should be added based on string boundaries
///
/// Simpler version for cases where we only have the text strings and font size
/// without full CharSpan information. This is useful when working with already
/// assembled text strings during text processing.
///
/// # Arguments
/// * `prev_text` - Previous text string
/// * `curr_text` - Current text string
/// * `horizontal_gap` - Horizontal gap in points between text boundaries
/// * `vertical_difference` - Vertical difference in points between text baselines
/// * `font_size` - Font size for threshold calculation
/// * `line_wrap_threshold` - Threshold for vertical differences indicating line wraps (typically 5.0)
///
/// # Returns
/// `true` if a space should be inserted
pub fn should_add_space_simple(
    prev_text: &str,
    curr_text: &str,
    horizontal_gap: f32,
    vertical_difference: f32,
    font_size: f32,
    line_wrap_threshold: f32,
) -> bool {
    // Calculate font-aware threshold for word boundary detection
    let min_word_boundary_gap = calculate_spacing_threshold(font_size);

    // Check if text strings already have spacing at boundaries
    let prev_text_ends_with_space = prev_text.ends_with(' ');
    let curr_text_starts_with_space = curr_text.starts_with(' ');
    let text_already_has_spacing = prev_text_ends_with_space || curr_text_starts_with_space;

    // Define line wrapping detection constants
    const NEGATIVE_GAP_LINE_WRAP_THRESHOLD: f32 = -10.0; // Large negative gap indicates line wrap
    const SMALL_VERTICAL_DIFF_THRESHOLD: f32 = 3.0; // Small Y difference for wrapped text

    // Determine if spacing is needed based on various conditions
    let horizontal_gap_indicates_word_boundary = horizontal_gap > min_word_boundary_gap;
    let vertical_gap_indicates_line_wrap = vertical_difference > line_wrap_threshold;
    let negative_gap_with_small_y_diff_suggests_line_wrap = horizontal_gap
        < NEGATIVE_GAP_LINE_WRAP_THRESHOLD
        && vertical_difference < SMALL_VERTICAL_DIFF_THRESHOLD;

    let spacing_conditions_met = horizontal_gap_indicates_word_boundary
        || vertical_gap_indicates_line_wrap
        || negative_gap_with_small_y_diff_suggests_line_wrap;

    if !spacing_conditions_met || text_already_has_spacing {
        return false;
    }

    // Don't insert space inside decimal numbers (digit.digit)
    if curr_text.starts_with(|c: char| c.is_ascii_digit()) {
        let prev_bytes = prev_text.as_bytes();
        let prev_len = prev_bytes.len();
        if prev_len >= 2
            && prev_bytes[prev_len - 1] == b'.'
            && prev_bytes[prev_len - 2].is_ascii_digit()
        {
            return false;
        }
        if prev_text == "." {
            return false;
        }
    }
    if curr_text == "." && prev_text.ends_with(|c: char| c.is_ascii_digit()) {
        return false;
    }

    // Don't insert space between adjacent digits on the same line.
    if horizontal_gap_indicates_word_boundary
        && !vertical_gap_indicates_line_wrap
        && !negative_gap_with_small_y_diff_suggests_line_wrap
    {
        let prev_ends_digit = prev_text.ends_with(|c: char| c.is_ascii_digit());
        let curr_starts_digit = curr_text.starts_with(|c: char| c.is_ascii_digit());
        if prev_ends_digit && curr_starts_digit {
            let digit_gap_limit = min_word_boundary_gap * DIGIT_ADJACENCY_GAP_MULTIPLE;
            if horizontal_gap < digit_gap_limit {
                return false;
            }
        }
    }

    true
}

/// Common single-character words that should not be treated as word fragments.
/// These are valid standalone English words that happen to be one character.
const COMMON_SINGLE_WORDS: [&str; 4] = ["a", "A", "i", "I"];

/// Common function words (prepositions, articles, conjunctions, pronouns, auxiliaries)
/// that frequently precede standalone variables/symbols in academic text.
/// When one of these words precedes a single character, the character is more likely
/// a standalone variable (e.g., "to m" where m=mass) than a word fragment.
///
/// NOTE: This list intentionally excludes archaic/rare words like "ye" that Hunspell
/// recognizes but are far more likely to be word fragments in modern documents.
/// Sorted alphabetically for binary search via `.binary_search().is_ok()`.
const FUNCTION_WORDS: [&str; 37] = [
    "am", "an", "and", "are", "as", "at", "be", "but", "by", "can", "did", "do", "for", "go",
    "had", "has", "he", "if", "in", "is", "it", "its", "may", "me", "my", "no", "not", "of", "on",
    "or", "so", "to", "up", "us", "was", "we", "while",
];

/// Check if a word part is likely a fragment rather than a standalone word.
///
/// A "fragment" is text that is probably part of a larger word that got split
/// during PDF extraction. This includes:
/// - Text that isn't a valid English word (e.g., "ye", "tions")
/// - Single characters that aren't common standalone words (e.g., "t", "D")
///
/// Used by word-join logic to decide whether to remove spaces between adjacent text.
#[cfg(feature = "correction-engine")]
pub fn is_word_fragment(word: &str) -> bool {
    use crate::font_analysis::dictionary::SmartCorrector;

    !SmartCorrector::is_valid_word(word)
        || (word.len() == 1 && !COMMON_SINGLE_WORDS.contains(&word))
}

/// Check if two adjacent word parts should be joined (space removed between them).
///
/// Returns `true` if the space between `word_before` and `word_after` should be
/// removed because they likely form a single word that was split during PDF extraction.
///
/// The function requires:
/// 1. Both parts are non-empty and the combined length is reasonable (2-15 chars)
/// 2. The joined result is a valid English word
/// 3. At least one part is a fragment (not a valid standalone word)
/// 4. Guard: a single-character `word_before` followed by a valid `word_after` is NOT
///    joined — the single char is likely a standalone symbol (e.g., math variable "D")
///    rather than a word fragment
#[cfg(feature = "correction-engine")]
pub fn should_join_fragments(word_before: &str, word_after: &str) -> bool {
    use crate::font_analysis::dictionary::SmartCorrector;

    if word_before.is_empty() || word_after.is_empty() {
        return false;
    }

    let combined_len = word_before.len() + word_after.len();
    if !(2..=15).contains(&combined_len) {
        return false;
    }

    let joined = format!("{}{}", word_before, word_after);
    if !SmartCorrector::is_valid_word(&joined) {
        return false;
    }

    let before_is_fragment = is_word_fragment(word_before);
    let after_is_fragment = is_word_fragment(word_after);

    // Guard: when one part is a single character and the other is a valid standalone
    // word, the single char is likely a symbol or variable, not a word fragment.
    //
    // word_before direction: "D" + "is" → single-char D before valid "is" → don't join
    //   Uses is_valid_word — any valid word after a single char triggers the guard.
    //
    // word_after direction: "to" + "m" → single-char m after function word "to" → don't join
    //   Uses FUNCTION_WORDS — only common function words trigger the guard.
    //   This avoids blocking legitimate joins like "ye" + "t" → "yet" where "ye" is
    //   a valid-but-archaic Hunspell word that's actually a PDF fragment.
    let word_before_lower = word_before.to_lowercase();
    let has_single_char_next_to_valid_word = (word_before.len() == 1
        && !COMMON_SINGLE_WORDS.contains(&word_before)
        && SmartCorrector::is_valid_word(word_after))
        || (word_after.len() == 1
            && !COMMON_SINGLE_WORDS.contains(&word_after)
            && FUNCTION_WORDS
                .binary_search(&word_before_lower.as_str())
                .is_ok());

    // Guard: don't join uppercase abbreviations/symbols with single uppercase letters.
    // In academic text, adjacent uppercase tokens like "OP" + "T" are separate symbols
    // (e.g., operator tree notation), not a split word like "ye" + "t" → "yet".
    let is_uppercase_symbol_pair = word_after.len() == 1
        && word_after.chars().all(|c| c.is_uppercase())
        && word_before.len() >= 2
        && word_before.chars().all(|c| c.is_uppercase());

    (before_is_fragment || after_is_fragment)
        && !has_single_char_next_to_valid_word
        && !is_uppercase_symbol_pair
}

/// Get the current font-aware spacing parameters for debugging/logging
///
/// This function provides access to the internal constants used for spacing
/// calculations, which can be useful for debugging, testing, or configuration.
///
/// # Returns
/// A tuple of (character_width_factor, word_boundary_threshold_factor)
/// - character_width_factor: How much of font size represents average character width (0.55)
/// - word_boundary_threshold_factor: Fraction of character width for word boundaries (0.16)
pub fn get_spacing_parameters() -> (f32, f32) {
    (AVERAGE_CHAR_WIDTH_FACTOR, WORD_BOUNDARY_THRESHOLD_FACTOR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spacing_threshold_calculation() {
        // For 9pt font (typical small text)
        let threshold_9pt = calculate_spacing_threshold(9.0);
        assert!((threshold_9pt - 0.792).abs() < 0.001); // 9 * 0.55 * 0.16 ≈ 0.792

        // For 12pt font (typical body text)
        let threshold_12pt = calculate_spacing_threshold(12.0);
        assert!((threshold_12pt - 1.056).abs() < 0.001); // 12 * 0.55 * 0.16 ≈ 1.056
    }

    #[test]
    fn test_should_add_space_simple_with_small_gap() {
        // Small gap should trigger space with font-aware threshold
        // This tests the specific "documentsD" -> "documents D" fix
        assert!(should_add_space_simple(
            "documents",
            "D",
            0.9,
            0.0,
            9.0,
            5.0
        ));
    }

    #[test]
    fn test_should_add_space_simple_gap_too_small() {
        // No space if gap is too small (below font-aware threshold)
        assert!(!should_add_space_simple("ab", "cd", 0.1, 0.0, 9.0, 5.0));
    }

    #[test]
    fn test_should_add_space_simple_vertical_line_wrap() {
        // Vertical difference should trigger space (line wrapping)
        assert!(should_add_space_simple(
            "line1", "line2", 0.0, 10.0, 9.0, 5.0
        ));
    }

    #[test]
    fn test_should_add_space_simple_existing_spaces() {
        // No space if text already has spacing at boundaries
        assert!(!should_add_space_simple(
            "text ", "more", 2.0, 0.0, 9.0, 5.0
        ));
        assert!(!should_add_space_simple(
            "text", " more", 2.0, 0.0, 9.0, 5.0
        ));
    }

    #[test]
    fn test_should_add_space_simple_negative_gap_line_wrap() {
        // Negative gap with small vertical difference suggests line wrap continuation
        assert!(should_add_space_simple(
            "end_of_line",
            "start_of_next",
            -15.0,
            2.0,
            12.0,
            5.0
        ));
    }

    #[test]
    fn test_no_space_in_decimal_number_simple() {
        // Case 1: "0." followed by "1" — no space (decimal number)
        assert!(!should_add_space_simple("0.", "1", 1.8, 0.0, 10.0, 5.0));
        assert!(!should_add_space_simple("28.", "4", 1.2, 0.0, 10.0, 5.0));
        // Case 2: standalone "." followed by digit — no space
        assert!(!should_add_space_simple(".", "1", 1.8, 0.0, 10.0, 5.0));
        // Case 3: digit followed by standalone "." — no space
        assert!(!should_add_space_simple("= 0", ".", 0.9, 0.0, 1.0, 5.0));
        assert!(!should_add_space_simple("0", ".", 0.9, 0.0, 1.0, 5.0));
    }

    #[test]
    fn test_space_after_sentence_period_simple() {
        // "sentence." followed by "Next" SHOULD get a space (sentence boundary, not decimal)
        assert!(should_add_space_simple(
            "sentence.",
            "Next",
            3.0,
            0.0,
            10.0,
            5.0
        ));
    }

    #[test]
    fn test_no_space_between_adjacent_digits_simple() {
        // "1" followed by "00" with small gap — no space (same number, e.g., "100")
        // threshold for 10pt = 10 * 0.55 * 0.16 = 0.88, digit limit = 0.88 * 3.0 = 2.64
        assert!(!should_add_space_simple("1", "00", 1.0, 0.0, 10.0, 5.0));
        assert!(!should_add_space_simple("1", "00", 2.0, 0.0, 10.0, 5.0));
        // Gap exceeding digit limit SHOULD get a space
        assert!(should_add_space_simple("1", "00", 3.0, 0.0, 10.0, 5.0));
        // Digit followed by non-digit should still get a space
        assert!(should_add_space_simple("1", "percent", 1.0, 0.0, 10.0, 5.0));
        // Non-digit followed by digit should still get a space
        assert!(should_add_space_simple("Chapter", "1", 1.0, 0.0, 10.0, 5.0));
    }

    #[test]
    fn test_get_spacing_parameters() {
        let (char_width_factor, boundary_factor) = get_spacing_parameters();
        assert_eq!(char_width_factor, 0.55);
        assert_eq!(boundary_factor, 0.16);
    }
}
