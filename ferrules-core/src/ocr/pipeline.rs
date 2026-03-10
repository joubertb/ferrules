//! End-to-end OCR pipeline: detection → cropping → recognition.
//!
//! Manages ORT session lifecycle and provides the top-level detect/recognize functions.
//! Sessions are protected by Mutex to prevent segfaults with CUDA/TensorRT EPs
//! (ORT's `unsafe impl Sync for Session` is known to be unsafe with non-CPU EPs).

use anyhow::Context;
use image::DynamicImage;
use std::sync::{Mutex, OnceLock};

use ort::{
    execution_providers::{CPUExecutionProvider, CUDAExecutionProvider, TensorRTExecutionProvider},
    session::{builder::GraphOptimizationLevel, Session},
};

use super::detection::{self, TextRegion};
use super::dictionary;
use super::recognition;

const DET_MODEL_BYTES: &[u8] = include_bytes!("../../../models/ocr/det_v3.onnx");
const REC_MODEL_BYTES: &[u8] = include_bytes!("../../../models/ocr/rec_v5_en.onnx");

struct OcrSession {
    session: Session,
    output_name: String,
}

static DET_SESSION: OnceLock<Mutex<OcrSession>> = OnceLock::new();
static REC_SESSION: OnceLock<Mutex<OcrSession>> = OnceLock::new();

fn build_session(model_bytes: &[u8]) -> anyhow::Result<OcrSession> {
    let session = Session::builder()
        .context("Failed to create ORT session builder")?
        .with_optimization_level(GraphOptimizationLevel::Level1)
        .context("Failed to set optimization level")?
        .with_execution_providers([
            TensorRTExecutionProvider::default().build(),
            CUDAExecutionProvider::default().build(),
            CPUExecutionProvider::default().build(),
        ])
        .context("Failed to set execution providers")?
        .commit_from_memory(model_bytes)
        .context("Failed to load ONNX model from memory")?;

    let output_name = session
        .outputs
        .first()
        .map(|o| o.name.clone())
        .context("ONNX model has no outputs — cannot determine output tensor name")?;

    Ok(OcrSession {
        session,
        output_name,
    })
}

fn get_det_session() -> anyhow::Result<&'static Mutex<OcrSession>> {
    DET_SESSION
        .get_or_try_init(|| build_session(DET_MODEL_BYTES).map(Mutex::new))
        .context("Failed to initialize OCR detection session")
}

fn get_rec_session() -> anyhow::Result<&'static Mutex<OcrSession>> {
    REC_SESSION
        .get_or_try_init(|| build_session(REC_MODEL_BYTES).map(Mutex::new))
        .context("Failed to initialize OCR recognition session")
}

/// Detect text regions in an image using DBNet.
pub fn detect_text_regions(image: &DynamicImage) -> anyhow::Result<Vec<TextRegion>> {
    let (orig_w, orig_h) = (image.width(), image.height());
    let (input_tensor, ratio_h, ratio_w) = detection::preprocess(image);

    let pred_h = input_tensor.shape()[2];
    let pred_w = input_tensor.shape()[3];

    // Run detection inference (mutex-protected for GPU EP safety)
    let det = get_det_session()?;
    let det_guard = det
        .lock()
        .map_err(|e| anyhow::anyhow!("Detection session lock poisoned: {}", e))?;

    let outputs = det_guard
        .session
        .run(ort::inputs![input_tensor.view()]?)
        .context("DBNet detection inference failed")?;

    let output_tensor = outputs.get(&det_guard.output_name).with_context(|| {
        format!(
            "Detection model output '{}' not found",
            det_guard.output_name
        )
    })?;

    let output_array = output_tensor
        .try_extract_tensor::<f32>()
        .context("Failed to extract detection output tensor")?;

    let pred = output_array
        .as_slice()
        .context("Detection output not contiguous")?;

    // Postprocess while still holding the lock — avoids copying ~3.5MB of pred data.
    // Postprocessing is fast CPU work (~1ms) so the added mutex hold time is negligible.
    let regions = detection::postprocess(pred, pred_h, pred_w, ratio_h, ratio_w, orig_w, orig_h);

    drop(outputs);
    drop(det_guard);

    Ok(regions)
}

/// Recognize text in cropped image regions using SVTR.
pub fn recognize_text(crops: &[DynamicImage]) -> anyhow::Result<Vec<(String, f32)>> {
    if crops.is_empty() {
        return Ok(vec![]);
    }

    let input_tensor = recognition::preprocess_batch(crops);
    let batch_size = input_tensor.shape()[0];

    // Run recognition inference (mutex-protected for GPU EP safety)
    let rec = get_rec_session()?;
    let rec_guard = rec
        .lock()
        .map_err(|e| anyhow::anyhow!("Recognition session lock poisoned: {}", e))?;

    let outputs = rec_guard
        .session
        .run(ort::inputs![input_tensor.view()]?)
        .context("SVTR recognition inference failed")?;

    let output_tensor = outputs.get(&rec_guard.output_name).with_context(|| {
        format!(
            "Recognition model output '{}' not found",
            rec_guard.output_name
        )
    })?;

    let output_array = output_tensor
        .try_extract_tensor::<f32>()
        .context("Failed to extract recognition output tensor")?;

    let shape = output_array.shape();
    let seq_len = shape[1];
    let num_classes = shape[2];

    // Verify num_classes matches our dictionary
    let expected_classes = dictionary::num_classes();
    if num_classes != expected_classes {
        tracing::error!(
            "Recognition model num_classes ({}) != dictionary size ({}). \
             CTC decode will produce incorrect text.",
            num_classes,
            expected_classes
        );
    }

    let output_data = output_array
        .as_slice()
        .context("Recognition output not contiguous")?;

    let output_owned: Vec<f32> = output_data.to_vec();
    drop(outputs);
    drop(rec_guard);

    Ok(recognition::decode_batch(
        &output_owned,
        batch_size,
        seq_len,
        num_classes,
    ))
}
