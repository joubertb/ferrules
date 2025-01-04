use std::path::Path;

use anyhow::Context;
use image::{imageops::FilterType, DynamicImage, Rgba};
use imageproc::drawing::draw_hollow_rect_mut;
use imageproc::rect::Rect;
use lazy_static::lazy_static;
use ndarray::{s, Array4, ArrayBase, Axis, Dim, OwnedRepr};
use ort::{
    execution_providers::CoreMLExecutionProvider,
    session::{builder::GraphOptimizationLevel, Session},
};

use crate::BBox;

#[derive(Debug, Default, Clone)]
struct LayoutBBox {
    bbox: BBox,
    label: &'static str,
    proba: f32,
}

lazy_static! {
    static ref ID2LABEL: [&'static str; 11] = [
        "Caption",
        "Footnote",
        "Title",
        "Formula",
        "List-item",
        "Page-footer",
        "Page-header",
        "Picture",
        "Section-header",
        "Table",
        "OTHER",
    ];
}

#[derive(Debug)]
pub struct ORTLayoutParser {
    session: Session,
}

impl ORTLayoutParser {
    pub fn new<P: AsRef<Path>>(model_path: P) -> anyhow::Result<Self> {
        let session = Session::builder()?
            .with_execution_providers([CoreMLExecutionProvider::default()
                .with_ane_only()
                .with_subgraphs()
                .build()])?
            .with_optimization_level(GraphOptimizationLevel::Level1)?
            .with_intra_threads(8)?
            .commit_from_file(model_path)?;
        Ok(Self { session })
    }
}

impl ORTLayoutParser {
    pub const REQUIRED_WIDTH: u32 = 1024;
    /// Required input image height.
    pub const REQUIRED_HEIGHT: u32 = 1024;

    // tensor: float32[1,15,21504]
    // 15 = 11 label classes + 4 bbox
    const OUTPUT_SIZE: [usize; 3] = [1, 15, 21504];

    const CONF_THRESHOLD: f32 = 0.8;
    const IOU_THRESHOLD: f32 = 0.7;

    pub fn parse_layout(&self, page_img: &DynamicImage) -> anyhow::Result<()> {
        let (img_width, img_height) = (page_img.width(), page_img.height());
        let input_name = self
            .session
            .inputs
            .first()
            .map(|i| &i.name)
            .context("can't find name for first input")?;

        let input = self.preprocess(page_img);

        let outputs = &self
            .session
            .run(ort::inputs![input_name=> input.clone()]?)?;

        let output_name = self
            .session
            .outputs
            .first()
            .map(|i| &i.name)
            .context("can't find name output input")?;

        let output_tensor = outputs
            .get(output_name)
            .context("can't get the value of first output")?
            .try_extract_tensor::<f32>()?;

        let output_tensor = output_tensor.to_shape(Self::OUTPUT_SIZE).unwrap();
        // TODO: Check safety here
        let output_tensor = output_tensor.t().to_owned();
        let bboxes = self.extract_bboxes(output_tensor, img_width, img_height);
        dbg!(&bboxes.len());

        let bboxes = nms(bboxes, Self::IOU_THRESHOLD);
        dbg!(&bboxes.len());

        let output_file = "test.png";
        draw_bboxes_and_save(&bboxes, page_img, output_file)?;

        Ok(())
    }

    fn extract_bboxes(
        &self,
        output: ArrayBase<OwnedRepr<f32>, Dim<[usize; 3]>>,
        original_width: u32,
        original_height: u32,
    ) -> Vec<LayoutBBox> {
        // Tensor shape: (21504,15)
        let mut result = Vec::new();
        let output = output.slice(s![.., .., 0]);
        for prediction in output.axis_iter(Axis(0)) {
            // Prediction dim: (15,) -> (4 bbox, 11 labels)
            let max_prob_idx = prediction
                .iter()
                .skip(4)
                .enumerate()
                .max_by(|(_, a), (_, b)| a.total_cmp(b))
                .map(|(max_idx, _)| max_idx)
                .unwrap();
            let proba = prediction[max_prob_idx + 4];
            if proba < Self::CONF_THRESHOLD {
                continue;
            }
            let label = ID2LABEL[max_prob_idx];
            let xc = prediction[0_usize] / Self::REQUIRED_WIDTH as f32 * (original_width as f32);
            let yc = prediction[1_usize] / Self::REQUIRED_HEIGHT as f32 * (original_height as f32);
            let w = prediction[2_usize] / Self::REQUIRED_WIDTH as f32 * (original_width as f32);
            let h = prediction[3_usize] / Self::REQUIRED_HEIGHT as f32 * (original_height as f32);
            // Change to (upper-left, lower-right)
            let x0 = xc - w / 2.0;
            let x1 = xc + w / 2.0;
            let y0 = yc - h / 2.0;
            let y1 = yc + h / 2.0;

            result.push(LayoutBBox {
                bbox: BBox { x0, y0, x1, y1 },
                proba,
                label,
            });
        }

        result
    }

    fn preprocess(&self, img: &DynamicImage) -> Array4<f32> {
        let resized_img = img.resize_exact(
            Self::REQUIRED_WIDTH,
            Self::REQUIRED_HEIGHT,
            FilterType::Triangle,
        );

        let mut input_tensor = Array4::zeros([
            1,
            3,
            Self::REQUIRED_HEIGHT as usize,
            Self::REQUIRED_WIDTH as usize,
        ]);
        for pixel in resized_img.into_rgba8().enumerate_pixels() {
            let x = pixel.0 as _;
            let y = pixel.1 as _;
            let [r, g, b, _] = pixel.2 .0;
            input_tensor[[0, 0, y, x]] = r as f32;
            input_tensor[[0, 1, y, x]] = g as f32;
            input_tensor[[0, 2, y, x]] = b as f32;
        }
        input_tensor
    }
}

fn nms(mut raw_bboxes: Vec<LayoutBBox>, iou_threshold: f32) -> Vec<LayoutBBox> {
    // TODO: can I sort this before ?
    raw_bboxes.sort_by(|r1, r2| r1.proba.total_cmp(&r2.proba));
    let mut result = Vec::new();
    while !raw_bboxes.is_empty() {
        result.push(raw_bboxes.first().unwrap().to_owned());

        raw_bboxes.retain(|rbbox| {
            let current_bbox = result.last().unwrap();
            rbbox.label == current_bbox.label && current_bbox.bbox.iou(&rbbox.bbox) < iou_threshold
        });
    }
    result
}

fn draw_bboxes_and_save(
    bboxes: &[LayoutBBox],
    page_img: &DynamicImage,
    out_path: impl AsRef<Path>,
) -> anyhow::Result<()> {
    // Convert the dynamic image to RGBA for in-place drawing.
    let mut out_img = page_img.to_rgba8();

    // Iterate over all bounding boxes and draw them.
    for layout_box in bboxes {
        let x0 = layout_box.bbox.x0 as i32;
        let y0 = layout_box.bbox.y0 as i32;
        let x1 = layout_box.bbox.x1 as i32;
        let y1 = layout_box.bbox.y1 as i32;

        // Determine rectangle width/height based on (x0, y0), (x1, y1).
        // Assuming x1 >= x0, y1 >= y0 in your data:
        let width = (x1 - x0).max(0) as u32;
        let height = (y1 - y0).max(0) as u32;

        // Create a Rect with top-left at (x0, y0) and size (width, height).
        let rect = Rect::at(x0, y0).of_size(width, height);

        // Draw a hollow rectangle (box outline only).
        // Use RGBA color [R, G, B, A] (e.g., red with full opacity).
        draw_hollow_rect_mut(&mut out_img, rect, Rgba([255, 0, 0, 255]));
    }

    // Save the image with the drawn bounding boxes.
    out_img.save(out_path)?;

    Ok(())
}
