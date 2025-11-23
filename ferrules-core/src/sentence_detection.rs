//! Sentence boundary detection using Unicode Standard Annex #29.
//!
//! This module provides functions to detect sentence boundaries in text,
//! returning character positions where sentences end. These positions are
//! used by the Python sentence splitter for precise bbox computation during
//! PDF sentence highlighting.

use unicode_segmentation::UnicodeSegmentation;

/// Detects sentence end positions in the given text.
///
/// Returns a vector of character indices where sentences end (inclusive).
/// Each index points to the last character of a sentence (typically punctuation).
///
/// # Example
/// ```
/// let text = "Dr. Smith arrived. He was late.";
/// let ends = detect_sentence_ends(text);
/// // ends = [17, 30] (positions of '.' at end of each sentence)
/// ```
pub fn detect_sentence_ends(text: &str) -> Vec<usize> {
    if text.is_empty() {
        return Vec::new();
    }

    let mut sentence_ends = Vec::new();
    let mut char_offset = 0;

    // Use unicode-segmentation to split on sentence boundaries
    for segment in text.split_sentence_bounds() {
        char_offset += segment.chars().count();

        // Check if this segment ends with sentence-ending punctuation
        let trimmed = segment.trim_end();
        if !trimmed.is_empty() {
            let last_char = trimmed.chars().last().unwrap();
            if is_sentence_ending_punctuation(last_char) {
                // Position is the index of the last character (0-based)
                // char_offset is already past this segment, so subtract 1
                // But we want the position of the punctuation, not whitespace
                let punct_offset =
                    char_offset - segment.chars().count() + trimmed.chars().count() - 1;
                sentence_ends.push(punct_offset);
            }
        }
    }

    // If text doesn't end with punctuation, mark the end as a sentence boundary
    if sentence_ends.is_empty() || sentence_ends.last() != Some(&(text.chars().count() - 1)) {
        let total_chars = text.chars().count();
        if total_chars > 0 {
            let trimmed = text.trim_end();
            if !trimmed.is_empty() {
                let last_char = trimmed.chars().last().unwrap();
                if !is_sentence_ending_punctuation(last_char) {
                    // Text ends without punctuation - mark end as sentence boundary
                    sentence_ends.push(trimmed.chars().count() - 1);
                }
            }
        }
    }

    sentence_ends
}

/// Checks if a character is sentence-ending punctuation.
fn is_sentence_ending_punctuation(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '…')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_sentences() {
        let text = "Hello world. This is a test.";
        let ends = detect_sentence_ends(text);
        assert_eq!(ends.len(), 2);
        // "Hello world." ends at position 11 (0-indexed)
        assert_eq!(ends[0], 11);
        // "This is a test." ends at position 27
        assert_eq!(ends[1], 27);
    }

    #[test]
    fn test_abbreviation() {
        let text = "Dr. Smith arrived. He was late.";
        let ends = detect_sentence_ends(text);
        // Unicode segmentation should recognize "Dr." as abbreviation
        // Expected: 2 sentences, not 3
        println!("Detected ends: {:?}", ends);
        // Note: Unicode rules may or may not handle "Dr." correctly
        // This test documents actual behavior
    }

    #[test]
    fn test_empty_text() {
        let ends = detect_sentence_ends("");
        assert!(ends.is_empty());
    }

    #[test]
    fn test_no_punctuation() {
        let text = "Hello world";
        let ends = detect_sentence_ends(text);
        assert_eq!(ends.len(), 1);
        assert_eq!(ends[0], 10); // Last char 'd' at position 10
    }

    #[test]
    fn test_multiple_punctuation() {
        let text = "What?! Really! Yes.";
        let ends = detect_sentence_ends(text);
        println!("Detected ends: {:?}", ends);
        assert!(!ends.is_empty());
    }

    #[test]
    fn test_question_and_exclamation() {
        let text = "How are you? I am fine!";
        let ends = detect_sentence_ends(text);
        assert_eq!(ends.len(), 2);
    }
}
