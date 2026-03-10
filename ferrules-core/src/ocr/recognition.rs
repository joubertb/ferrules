//! SVTR text recognition using PP-OCRv5 English recognition model.
//!
//! Pipeline: crop text regions → resize/normalize → ORT inference → CTC decode.

use image::DynamicImage;
use ndarray::Array4;

use super::dictionary;

/// Fixed input height for PP-OCRv5 recognition model.
const REC_IMG_HEIGHT: u32 = 32;
/// Maximum input width (wider crops are clamped).
const REC_MAX_WIDTH: u32 = 320;

/// Preprocess a batch of cropped text images for recognition.
///
/// Returns input tensor with shape [batch, 3, 32, max_w].
pub fn preprocess_batch(crops: &[DynamicImage]) -> Array4<f32> {
    if crops.is_empty() {
        return Array4::zeros((0, 3, REC_IMG_HEIGHT as usize, 1));
    }

    // Compute max width from aspect ratios
    let mut max_w = 0u32;
    let mut resized_widths = Vec::with_capacity(crops.len());

    for crop in crops {
        let (w, h) = (crop.width(), crop.height());
        if h == 0 {
            resized_widths.push(REC_IMG_HEIGHT); // fallback
            continue;
        }
        let ratio = w as f32 / h as f32;
        let resized_w = (REC_IMG_HEIGHT as f32 * ratio).ceil() as u32;
        let resized_w = resized_w.min(REC_MAX_WIDTH).max(1);
        resized_widths.push(resized_w);
        max_w = max_w.max(resized_w);
    }

    max_w = max_w.max(1);

    // Create zero-filled tensor (padding = 0.0)
    let batch_size = crops.len();
    let mut tensor = Array4::<f32>::zeros((batch_size, 3, REC_IMG_HEIGHT as usize, max_w as usize));

    for (i, crop) in crops.iter().enumerate() {
        let resized_w = resized_widths[i];

        // Resize to (resized_w, REC_IMG_HEIGHT) and convert to RGB8 for fast access
        let resized = crop
            .resize_exact(
                resized_w,
                REC_IMG_HEIGHT,
                image::imageops::FilterType::Triangle,
            )
            .to_rgb8();

        // Normalize: (pixel / 255.0 - 0.5) / 0.5 = pixel / 127.5 - 1.0
        for (x, y, pixel) in resized.enumerate_pixels() {
            for c in 0..3 {
                tensor[[i, c, y as usize, x as usize]] = pixel[c] as f32 / 127.5 - 1.0;
            }
        }
        // Right side stays 0.0 (zero-padding)
    }

    tensor
}

/// Decode recognition model output into text strings with confidence scores.
///
/// `output` shape: [batch, seq_len, num_classes] as flat slice.
pub fn decode_batch(
    output: &[f32],
    batch_size: usize,
    seq_len: usize,
    num_classes: usize,
) -> Vec<(String, f32)> {
    let mut results = Vec::with_capacity(batch_size);
    let step = seq_len * num_classes;

    for i in 0..batch_size {
        let logits = &output[i * step..(i + 1) * step];
        let (text, confidence) = dictionary::ctc_greedy_decode(logits, seq_len, num_classes);
        results.push((text, confidence));
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preprocess_empty() {
        let tensor = preprocess_batch(&[]);
        assert_eq!(tensor.shape()[0], 0);
    }

    #[test]
    fn test_preprocess_single() {
        // Create a 100x20 test image (wider than tall)
        let img = DynamicImage::new_rgb8(100, 20);
        let tensor = preprocess_batch(&[img]);

        assert_eq!(tensor.shape()[0], 1); // batch=1
        assert_eq!(tensor.shape()[1], 3); // channels
        assert_eq!(tensor.shape()[2], 32); // height
                                           // Width should be ceil(32 * 100/20) = 160
        assert_eq!(tensor.shape()[3], 160);
    }

    #[test]
    fn test_preprocess_normalization() {
        // Black image (all zeros) should normalize to -1.0
        let img = DynamicImage::new_rgb8(10, 10);
        let tensor = preprocess_batch(&[img]);
        assert!((tensor[[0, 0, 0, 0]] - (-1.0)).abs() < 1e-6);
    }
}
