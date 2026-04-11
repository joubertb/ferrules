//! End-to-end OCR pipeline: detection → cropping → recognition.
//!
//! Manages ORT session lifecycle and provides the top-level detect/recognize functions.
//! Sessions are protected by Mutex to prevent segfaults with CUDA/TensorRT EPs
//! (ORT's `unsafe impl Sync for Session` is known to be unsafe with non-CPU EPs).

use anyhow::Context;
use image::DynamicImage;
use ndarray::Array4;
use once_cell::sync::OnceCell;
use std::sync::Mutex;

use ort::{
    execution_providers::{CPUExecutionProvider, CUDAExecutionProvider, TensorRTExecutionProvider},
    session::{builder::GraphOptimizationLevel, Session},
    value::TensorRef,
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

static DET_SESSION: OnceCell<Mutex<OcrSession>> = OnceCell::new();
static REC_SESSION: OnceCell<Mutex<OcrSession>> = OnceCell::new();
/// Recognition model's fixed input height, read from the ONNX model at init time.
static REC_INPUT_HEIGHT: OnceCell<u32> = OnceCell::new();

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

/// Extract the fixed height dimension from a recognition model's input shape.
///
/// Expected input shape: [batch, 3, height, width] where batch and width are
/// dynamic (-1) and height is fixed. Returns an error if the shape doesn't
/// match this pattern.
fn extract_rec_input_height(session: &Session) -> anyhow::Result<u32> {
    let input = session
        .inputs
        .first()
        .context("Recognition model has no inputs")?;

    let dims = input
        .input_type
        .tensor_shape()
        .context("Recognition model input is not a tensor")?;
    anyhow::ensure!(
        dims.len() == 4,
        "Recognition model input must be 4D [batch, channels, height, width], got {:?}",
        dims
    );
    anyhow::ensure!(
        dims[1] == 3,
        "Recognition model input channels must be 3, got {}",
        dims[1]
    );
    let height = dims[2];
    anyhow::ensure!(
        height > 0,
        "Recognition model input height is dynamic (-1) — expected a fixed dimension. \
         Cannot auto-detect preprocessing height."
    );
    Ok(height as u32)
}

fn get_det_session() -> anyhow::Result<&'static Mutex<OcrSession>> {
    DET_SESSION
        .get_or_try_init(|| build_session(DET_MODEL_BYTES).map(Mutex::new))
        .context("Failed to initialize OCR detection session")
}

fn get_rec_session() -> anyhow::Result<&'static Mutex<OcrSession>> {
    REC_SESSION
        .get_or_try_init(|| -> anyhow::Result<Mutex<OcrSession>> {
            let mut ocr_session = build_session(REC_MODEL_BYTES)?;

            let model_height = extract_rec_input_height(&ocr_session.session)?;
            REC_INPUT_HEIGHT.get_or_init(|| model_height);

            // Validate dictionary size matches model output by running a dummy inference.
            // The model's last output dimension = num_classes, which must equal our dictionary size.
            let model_num_classes = {
                let dummy = Array4::<f32>::zeros((1, 3, model_height as usize, 16));
                let dummy_out = ocr_session
                    .session
                    .run(ort::inputs![TensorRef::from_array_view(dummy.view())?])
                    .context("Validation: dummy inference failed")?;
                let dummy_tensor = dummy_out
                    .get(&ocr_session.output_name)
                    .context("Validation: output not found")?;
                let (dummy_shape, _dummy_data) = dummy_tensor
                    .try_extract_tensor::<f32>()
                    .context("Validation: failed to extract tensor")?;
                dummy_shape[2] as usize
            };
            let dict_num_classes = dictionary::num_classes();
            anyhow::ensure!(
                model_num_classes == dict_num_classes,
                "Recognition model num_classes ({}) != dictionary size ({}). \
                 Model and dict_en.txt are out of sync.",
                model_num_classes,
                dict_num_classes,
            );

            tracing::info!(
                "OCR recognition model loaded: input height={}px, num_classes={}",
                model_height,
                model_num_classes,
            );

            Ok(Mutex::new(ocr_session))
        })
        .context("Failed to initialize OCR recognition session")
}

/// Get the recognition model's required input height, initializing the session if needed.
pub fn rec_input_height() -> anyhow::Result<u32> {
    // Ensure the session (and REC_INPUT_HEIGHT) are initialized
    let _ = get_rec_session()?;
    Ok(*REC_INPUT_HEIGHT
        .get()
        .expect("REC_INPUT_HEIGHT must be set after get_rec_session succeeds"))
}

/// Detect text regions in an image using DBNet.
pub fn detect_text_regions(image: &DynamicImage) -> anyhow::Result<Vec<TextRegion>> {
    let (orig_w, orig_h) = (image.width(), image.height());
    let (input_tensor, ratio_h, ratio_w) = detection::preprocess(image);

    let pred_h = input_tensor.shape()[2];
    let pred_w = input_tensor.shape()[3];

    // Run detection inference (mutex-protected for GPU EP safety)
    let det = get_det_session()?;
    let mut det_guard = det
        .lock()
        .map_err(|e| anyhow::anyhow!("Detection session lock poisoned: {}", e))?;

    let det_output_name = det_guard.output_name.clone();
    let outputs = det_guard
        .session
        .run(ort::inputs![TensorRef::from_array_view(input_tensor.view())?])
        .context("DBNet detection inference failed")?;

    let output_tensor = outputs.get(&det_output_name).with_context(|| {
        format!("Detection model output '{}' not found", det_output_name)
    })?;

    let (_output_shape, output_data) = output_tensor
        .try_extract_tensor::<f32>()
        .context("Failed to extract detection output tensor")?;

    let pred: &[f32] = output_data;

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

    let input_tensor = recognition::preprocess_batch(crops)?;
    let batch_size = input_tensor.shape()[0];

    // Run recognition inference (mutex-protected for GPU EP safety)
    let rec = get_rec_session()?;
    let mut rec_guard = rec
        .lock()
        .map_err(|e| anyhow::anyhow!("Recognition session lock poisoned: {}", e))?;

    let rec_output_name = rec_guard.output_name.clone();
    let outputs = rec_guard
        .session
        .run(ort::inputs![TensorRef::from_array_view(input_tensor.view())?])
        .context("SVTR recognition inference failed")?;

    let output_tensor = outputs.get(&rec_output_name).with_context(|| {
        format!("Recognition model output '{}' not found", rec_output_name)
    })?;

    let (output_shape, output_data) = output_tensor
        .try_extract_tensor::<f32>()
        .context("Failed to extract recognition output tensor")?;

    let seq_len = output_shape[1] as usize;
    let num_classes = output_shape[2] as usize;

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
