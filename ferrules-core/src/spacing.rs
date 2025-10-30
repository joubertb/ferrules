/// Font-aware text spacing utilities
///
/// This module provides consistent spacing logic across all text processing pipelines
/// to ensure proper word boundaries are detected in PDF text extraction.
use crate::entities::CharSpan;

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
    spacing_conditions_met && !spans_already_have_spacing
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

    // Only add space if conditions are met AND text doesn't already have spacing
    spacing_conditions_met && !text_already_has_spacing
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
    fn test_get_spacing_parameters() {
        let (char_width_factor, boundary_factor) = get_spacing_parameters();
        assert_eq!(char_width_factor, 0.55);
        assert_eq!(boundary_factor, 0.16);
    }
}
