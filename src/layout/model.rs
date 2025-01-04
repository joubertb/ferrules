use std::{collections::HashMap, path::Path};

use anyhow::Context;
use image::{imageops::FilterType, DynamicImage};
use lazy_static::lazy_static;
use ndarray::{Array, Array3, Array4, ArrayBase, CowRepr, Dim, OwnedRepr};
use ort::{
    execution_providers::CoreMLExecutionProvider,
    session::{builder::GraphOptimizationLevel, Session},
};

use super::LayoutResult;

lazy_static! {
    static ref ID2LABEL: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert("0", "Caption");
        m.insert("1", "Footnote");
        m.insert("2", "Formula");
        m.insert("3", "List-item");
        m.insert("4", "Page-footer");
        m.insert("5", "Page-header");
        m.insert("6", "Picture");
        m.insert("7", "Section-header");
        m.insert("8", "Table");
        m.insert("9", "Text");
        m.insert("10", "Title");
        m
    };
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
    pub const REQUIRED_WIDTH: u32 = 768;
    /// Required input image height.
    pub const REQUIRED_HEIGHT: u32 = 1024;

    const OUTPUT_SIZE: [usize; 3] = [1, 16128, 16];

    pub fn parse_layout(&self, page_img: &DynamicImage) -> anyhow::Result<LayoutResult> {
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

        Ok(LayoutResult {})
    }

    fn extract_bboxes<'o>(&self, output: Array3<CowRepr<'o, f32>>, p6: bool) {
        let strides = if !p6 {
            vec![8, 16, 32]
        } else {
            vec![8, 16, 32, 64]
        };
    }

    fn preprocess(&self, img: &DynamicImage) -> Array4<f32> {
        let (img_width, img_height) = (img.width(), img.height());

        let mut padded_img: ArrayBase<OwnedRepr<f32>, Dim<[usize; 4]>> = Array::ones((
            1,
            3,
            Self::REQUIRED_HEIGHT as usize,
            Self::REQUIRED_WIDTH as usize,
        )) * 114_f32;

        let r: f64 = f64::min(
            Self::REQUIRED_HEIGHT as f64 / img_height as f64,
            Self::REQUIRED_WIDTH as f64 / img_width as f64,
        );

        let resized_img = img.resize_exact(
            (img_width as f64 * r) as u32,
            (img_height as f64 * r) as u32,
            FilterType::Triangle,
        );

        for pixel in resized_img.into_rgba8().enumerate_pixels() {
            let x = pixel.0 as _;
            let y = pixel.1 as _;
            let [r, g, b, _] = pixel.2 .0;
            padded_img[[0, 0, y, x]] = r as f32;
            padded_img[[0, 1, y, x]] = g as f32;
            padded_img[[0, 2, y, x]] = b as f32;
        }

        padded_img
    }
}
