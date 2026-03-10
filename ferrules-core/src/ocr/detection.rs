//! DBNet text detection using PP-OCRv3 detection model.
//!
//! Pipeline: preprocess image → ORT inference → threshold → find contours → unclip → bboxes.

use image::{DynamicImage, GrayImage};
use imageproc::contours::{find_contours, BorderType};
use ndarray::Array4;

use crate::entities::BBox;

// DBNet post-processing constants (canonical PaddleOCR defaults)
const DET_THRESH: f32 = 0.3;
const DET_BOX_THRESH: f32 = 0.5; // slightly relaxed from canonical 0.7 for better recall
const DET_UNCLIP_RATIO: f32 = 2.0;
const DET_MIN_SIZE: f32 = 3.0;
const DET_MAX_CANDIDATES: usize = 1000;

// Resize constants
const LIMIT_SIDE_LEN: u32 = 960;

/// A detected text region with bounding box and detection confidence.
#[derive(Debug, Clone)]
pub struct TextRegion {
    pub bbox: BBox,
    pub score: f32,
}

/// Preprocess an image for DBNet detection.
///
/// Returns (input_tensor, ratio_h, ratio_w) where ratios map from resized → original coordinates.
pub fn preprocess(image: &DynamicImage) -> (Array4<f32>, f32, f32) {
    let (orig_w, orig_h) = (image.width(), image.height());

    // Compute resize dimensions
    let max_side = orig_w.max(orig_h);
    let ratio = if max_side > LIMIT_SIDE_LEN {
        LIMIT_SIDE_LEN as f32 / max_side as f32
    } else {
        1.0
    };

    let resize_h = ((orig_h as f32 * ratio / 32.0).round() as u32 * 32).max(32);
    let resize_w = ((orig_w as f32 * ratio / 32.0).round() as u32 * 32).max(32);

    let ratio_h = orig_h as f32 / resize_h as f32;
    let ratio_w = orig_w as f32 / resize_w as f32;

    // Resize and convert to RGB8 for fast pixel access
    let resized = image
        .resize_exact(resize_w, resize_h, image::imageops::FilterType::Triangle)
        .to_rgb8();

    // Normalize and convert to NCHW tensor
    let mean = [0.485f32, 0.456, 0.406];
    let std = [0.229f32, 0.224, 0.225];

    let mut tensor = Array4::<f32>::zeros((1, 3, resize_h as usize, resize_w as usize));
    for (x, y, pixel) in resized.enumerate_pixels() {
        for c in 0..3 {
            tensor[[0, c, y as usize, x as usize]] = (pixel[c] as f32 / 255.0 - mean[c]) / std[c];
        }
    }

    (tensor, ratio_h, ratio_w)
}

/// Post-process DBNet output probability map into text regions.
///
/// `pred` is the raw model output: shape [1, 1, H, W] flattened.
/// `pred_h` and `pred_w` are the spatial dimensions.
/// `ratio_h` and `ratio_w` map from model coordinates to original image coordinates.
/// `orig_w` and `orig_h` are the original image dimensions for clamping.
pub fn postprocess(
    pred: &[f32],
    pred_h: usize,
    pred_w: usize,
    ratio_h: f32,
    ratio_w: f32,
    orig_w: u32,
    orig_h: u32,
) -> Vec<TextRegion> {
    if pred_w == 0 || pred_h == 0 {
        return vec![];
    }

    // Step 1: Binarize to GrayImage (from_raw avoids per-pixel bounds checks)
    let pixels: Vec<u8> = pred
        .iter()
        .map(|&v| if v > DET_THRESH { 255u8 } else { 0u8 })
        .collect();
    let bitmap = GrayImage::from_raw(pred_w as u32, pred_h as u32, pixels)
        .expect("bitmap dimensions match pred slice length");

    // Step 2: Find contours
    let contours = find_contours::<i32>(&bitmap);

    let mut regions = Vec::new();
    let mut candidate_count = 0;

    for contour in &contours {
        if contour.border_type != BorderType::Outer {
            continue;
        }
        if contour.points.len() < 3 {
            continue;
        }
        if candidate_count >= DET_MAX_CANDIDATES {
            break;
        }
        candidate_count += 1;

        // Step 3a: Compute axis-aligned bounding box of contour
        let (mut min_x, mut min_y) = (i32::MAX, i32::MAX);
        let (mut max_x, mut max_y) = (i32::MIN, i32::MIN);
        for p in &contour.points {
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x);
            max_y = max_y.max(p.y);
        }

        let box_w = (max_x - min_x) as f32;
        let box_h = (max_y - min_y) as f32;
        let sside = box_w.min(box_h);

        if sside < DET_MIN_SIZE {
            continue;
        }

        // Step 3b: Compute box score (mean probability inside bbox)
        let score = compute_box_score(
            pred,
            pred_w,
            min_x.max(0) as usize,
            min_y.max(0) as usize,
            (max_x as usize).min(pred_w - 1),
            (max_y as usize).min(pred_h - 1),
        );

        if score < DET_BOX_THRESH {
            continue;
        }

        // Step 3c: Unclip (expand) the bounding box
        let area = box_w * box_h;
        let perimeter = 2.0 * (box_w + box_h);
        let distance = area * DET_UNCLIP_RATIO / perimeter;

        let exp_min_x = (min_x as f32 - distance).max(0.0);
        let exp_min_y = (min_y as f32 - distance).max(0.0);
        let exp_max_x = (max_x as f32 + distance).min(pred_w as f32 - 1.0);
        let exp_max_y = (max_y as f32 + distance).min(pred_h as f32 - 1.0);

        // Check expanded box is still valid
        let exp_w = exp_max_x - exp_min_x;
        let exp_h = exp_max_y - exp_min_y;
        if exp_w.min(exp_h) < DET_MIN_SIZE + 2.0 {
            continue;
        }

        // Step 3d: Scale to original image coordinates
        let bbox = BBox {
            x0: (exp_min_x * ratio_w).clamp(0.0, orig_w as f32),
            y0: (exp_min_y * ratio_h).clamp(0.0, orig_h as f32),
            x1: (exp_max_x * ratio_w).clamp(0.0, orig_w as f32),
            y1: (exp_max_y * ratio_h).clamp(0.0, orig_h as f32),
        };

        // Filter tiny boxes in original coordinates
        if (bbox.x1 - bbox.x0) < 4.0 || (bbox.y1 - bbox.y0) < 4.0 {
            continue;
        }

        regions.push(TextRegion { bbox, score });
    }

    // Sort: top-to-bottom, left-to-right within ~10px Y tolerance
    regions.sort_by(|a, b| {
        let y_diff = a.bbox.y0 - b.bbox.y0;
        if y_diff.abs() < 10.0 {
            a.bbox
                .x0
                .partial_cmp(&b.bbox.x0)
                .unwrap_or(std::cmp::Ordering::Equal)
        } else {
            a.bbox
                .y0
                .partial_cmp(&b.bbox.y0)
                .unwrap_or(std::cmp::Ordering::Equal)
        }
    });

    regions
}

/// Compute mean probability value inside a bounding box region.
fn compute_box_score(
    pred: &[f32],
    pred_w: usize,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
) -> f32 {
    let mut sum = 0.0f32;
    let mut count = 0u32;
    for y in y0..=y1 {
        for x in x0..=x1 {
            sum += pred[y * pred_w + x];
            count += 1;
        }
    }
    if count > 0 {
        sum / count as f32
    } else {
        0.0
    }
}
