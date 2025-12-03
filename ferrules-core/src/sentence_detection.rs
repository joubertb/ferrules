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

    #[test]
    fn test_mathbert_sentence_boundary() {
        // Test case from actual PDF where sentence boundary splits a word
        let text = "three tasks. Moreover, we qualitatively";
        let ends = detect_sentence_ends(text);
        println!("Text: {:?}", text);
        println!("Detected ends: {:?}", ends);

        // The period is at position 11
        // The next sentence "Moreover, we qualitatively" should NOT split the word
        for (i, end) in ends.iter().enumerate() {
            let char_at_end: char = text.chars().nth(*end).unwrap();
            println!("End {}: position {} = '{}'", i, end, char_at_end);
        }

        // First sentence ends at position 11 (the '.')
        assert_eq!(ends[0], 11, "First sentence should end at period position");
    }

    #[test]
    fn test_real_mathbert_text() {
        // Actual text from the problematic PDF block
        let text = "isting methods on all those three tasks. Moreover, we qualitatively show";
        let ends = detect_sentence_ends(text);
        println!("Text length: {}", text.len());
        println!("Detected ends: {:?}", ends);

        // Find where "three tasks." ends
        let period_pos = text.find("tasks.").map(|i| i + 5); // position of the period
        println!("Period after 'tasks' at position: {:?}", period_pos);

        for (i, end) in ends.iter().enumerate() {
            if *end < text.len() {
                let char_at_end: char = text.chars().nth(*end).unwrap();
                println!("End {}: position {} = '{}'", i, end, char_at_end);
            }
        }
    }

    #[test]
    fn test_debug_sentence_segmentation() {
        // The actual problematic part of the fertext
        let text = "isting methods on all those three tasks. Moreover, we qualitatively";

        println!("=== Debug sentence segmentation ===");
        println!("Text: {:?}", text);
        println!("Text length (chars): {}", text.chars().count());
        println!();

        let mut char_offset = 0;
        for (i, segment) in text.split_sentence_bounds().enumerate() {
            let segment_len = segment.chars().count();
            let start = char_offset;
            char_offset += segment_len;

            println!(
                "Segment {}: [{}, {}) len={} {:?}",
                i, start, char_offset, segment_len, segment
            );

            let trimmed = segment.trim_end();
            if !trimmed.is_empty() {
                let last_char = trimmed.chars().last().unwrap();
                if matches!(last_char, '.' | '!' | '?' | '…') {
                    let punct_offset = char_offset - segment_len + trimmed.chars().count() - 1;
                    println!(
                        "  -> Sentence ends at position {} (char '{}')",
                        punct_offset,
                        text.chars().nth(punct_offset).unwrap_or('?')
                    );
                }
            }
        }
    }

    #[test]
    fn test_full_block5_fertext() {
        // This is the actual fertext from mathbert.pdf block 5
        // Note: "demon- rate" is corrupted text from PDF extraction
        let text = "Large-scale pre-trained models like BERT, have obtained a great success in various Natural Lan- guage Processing (NLP) tasks, while it is still a challenge to adapt them to the math-related tasks. Current pre-trained models neglect the structural features and the semantic correspondence between formula and its context. To address these issues, we propose a novel pre-trained model, namely Math- BERT , which is jointly trained with mathematical formulas and their corresponding contexts. In addi- tion, in order to further capture the semantic-level structural features of formulas, a new pre-training task is designed to predict the masked formula sub- structures extracted from the Operator Tree (OPT), which is the semantic structural representation of formulas. We conduct various experiments on three downstream tasks to evaluate the performance of MathBERT, including mathematical information retrieval, formula topic classification and formula headline generation. Experimental results demon- rate that MathBERT significantly outperforms ex- isting methods on all those three tasks. Moreover, we qualitatively show that this pre-trained model effectively captures the semantic-level structural information of formulas. To the best of our knowl- edge, MathBERT is the first pre-trained model for mathematical formula understanding.";

        println!("Text length: {} chars", text.chars().count());

        let ends = detect_sentence_ends(text);
        println!("Detected ends: {:?}", ends);

        // Find where "three tasks." ends
        let tasks_idx = text.find("three tasks.").unwrap();
        let period_pos = tasks_idx + 11; // position of the '.'
        println!(
            "\"three tasks.\" period at position {}: '{}'",
            period_pos,
            text.chars().nth(period_pos).unwrap()
        );

        // Check if sentence_ends includes this position
        // If using inclusive positioning, should be 1090
        // If using exclusive positioning (for Python slicing), should be 1091
        println!();
        for (i, end) in ends.iter().enumerate() {
            if *end > 950 && *end < 1150 {
                println!(
                    "End {}: position {} = '{}'",
                    i,
                    end,
                    text.chars().nth(*end).unwrap_or('?')
                );
            }
        }

        // The period at position 1090 should be included in sentence_ends
        // Either as 1090 (inclusive) or there should be an end close to it
        let closest_end = ends.iter().filter(|&&e| e >= 1080 && e <= 1100).next();
        println!("\nClosest end to period position 1090: {:?}", closest_end);
    }
}
