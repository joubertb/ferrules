//! PaddleOCR PP-OCRv5 English character dictionary for CTC decoding.
//!
//! Character list layout:
//!   Index 0 = CTC blank token
//!   Index 1..=N = dictionary characters (from dict_en.txt)
//!   Index N+1 = space character (appended)
//!
//! The dict_en.txt file contains 436 characters covering ASCII, accented Latin,
//! Greek, math symbols, currency symbols, and various Unicode characters.

use std::sync::OnceLock;

const DICT_RAW: &str = include_str!("../../../models/ocr/dict_en.txt");

/// Character list for CTC decoding.
/// Index 0 = blank, 1..436 = dict chars, 437 = space.
/// Total: 438 entries matching model output num_classes.
pub fn char_list() -> &'static Vec<char> {
    static CHAR_LIST: OnceLock<Vec<char>> = OnceLock::new();
    CHAR_LIST.get_or_init(|| {
        let mut chars: Vec<char> = vec!['\0']; // CTC blank at index 0
        for line in DICT_RAW.lines() {
            if let Some(ch) = line.chars().next() {
                chars.push(ch);
            }
        }
        chars.push(' '); // space appended at end
        chars
    })
}

/// Number of classes in the recognition model output.
pub fn num_classes() -> usize {
    char_list().len()
}

/// CTC greedy decode: collapse repeated characters, remove blanks.
///
/// `logits` shape: [seq_len, num_classes] stored as flat slice.
/// Returns (decoded_text, mean_confidence).
pub fn ctc_greedy_decode(logits: &[f32], seq_len: usize, num_classes: usize) -> (String, f32) {
    if seq_len == 0 || num_classes == 0 {
        return (String::new(), 0.0);
    }

    let chars = char_list();
    let mut prev_idx = usize::MAX;
    let mut text = String::new();
    let mut conf_sum = 0.0f32;
    let mut conf_count = 0u32;

    for t in 0..seq_len {
        let row = &logits[t * num_classes..(t + 1) * num_classes];

        // Argmax
        let (max_idx, &max_val) = row
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap();

        // Skip if same as previous (collapse duplicates) or if blank (index 0)
        if max_idx != prev_idx && max_idx != 0 {
            if max_idx < chars.len() {
                text.push(chars[max_idx]);
                conf_sum += max_val;
                conf_count += 1;
            }
        }

        prev_idx = max_idx;
    }

    let confidence = if conf_count > 0 {
        conf_sum / conf_count as f32
    } else {
        0.0
    };
    (text, confidence)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_char_list_structure() {
        let chars = char_list();
        // First entry is blank
        assert_eq!(chars[0], '\0');
        // Last entry is space
        assert_eq!(*chars.last().unwrap(), ' ');
        // Dict has 436 lines, so total = 438
        assert_eq!(chars.len(), 438);
        // First dict char is '0'
        assert_eq!(chars[1], '0');
        // Digits 0-9 at indices 1-10
        assert_eq!(chars[10], '9');
    }

    #[test]
    fn test_ctc_decode_simple() {
        let chars = char_list();
        let nc = chars.len();
        let seq_len = 5;

        // Find indices for 'H' and 'i' dynamically from the character list
        let h_idx = chars
            .iter()
            .position(|&c| c == 'H')
            .expect("'H' not in char_list");
        let i_idx = chars
            .iter()
            .position(|&c| c == 'i')
            .expect("'i' not in char_list");

        let mut logits = vec![0.0f32; seq_len * nc];

        // t=0: blank (index 0)
        logits[0] = 10.0;
        // t=1: H
        logits[1 * nc + h_idx] = 10.0;
        // t=2: H repeated (should be collapsed)
        logits[2 * nc + h_idx] = 10.0;
        // t=3: i
        logits[3 * nc + i_idx] = 10.0;
        // t=4: blank
        logits[4 * nc] = 10.0;

        let (text, _conf) = ctc_greedy_decode(&logits, seq_len, nc);
        assert_eq!(text, "Hi");
    }
}
