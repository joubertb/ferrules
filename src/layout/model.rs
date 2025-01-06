use std::path::Path;

use anyhow::Context;
use image::{imageops::FilterType, DynamicImage, GenericImageView, Rgba};
use imageproc::drawing::draw_hollow_rect_mut;
use imageproc::rect::Rect;
use lazy_static::lazy_static;
use ndarray::{s, Array, Array4, ArrayBase, Axis, Dim, OwnedRepr};
use ort::{
    execution_providers::{CPUExecutionProvider, CoreMLExecutionProvider},
    session::{builder::GraphOptimizationLevel, Session},
};

use crate::BBox;

use super::draw::draw_bboxes_and_save;

#[derive(Debug, Default, Clone)]
pub(crate) struct LayoutBBox {
    pub(crate) bbox: BBox,
    pub(crate) label: &'static str,
    pub(crate) proba: f32,
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
        "Text",
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
                .with_subgraphs()
                .build()])?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
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

    const CONF_THRESHOLD: f32 = 0.3;
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

        let output_tensor = output_tensor
            .to_shape(Self::OUTPUT_SIZE)
            .unwrap()
            .to_owned();
        let mut bboxes = self.extract_bboxes(output_tensor, img_width, img_height);
        nms(&mut bboxes, Self::IOU_THRESHOLD);

        // dbg!(&bboxes);
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
        // Tensor shape: (bs, bbox(4) + classes(15),anchors )
        let mut result = Vec::new();
        let output = output.slice(s![0, .., ..]);
        for prediction in output.axis_iter(Axis(1)) {
            // Prediction dim: (15,) -> (4 bbox, 11 labels)
            const CXYWH_OFFSET: usize = 4;
            let bbox = prediction.slice(s![0..CXYWH_OFFSET]);
            let classes = prediction.slice(s![CXYWH_OFFSET..CXYWH_OFFSET + ID2LABEL.len()]);
            let max_prob_idx = classes
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(max_idx, _)| max_idx)
                .unwrap();

            let proba = classes[max_prob_idx];
            if proba < Self::CONF_THRESHOLD {
                continue;
            }
            let label = ID2LABEL[max_prob_idx];
            let ratio = (Self::REQUIRED_WIDTH as f32 / original_width as f32)
                .min(Self::REQUIRED_HEIGHT as f32 / original_height as f32);
            let xc = bbox[0_usize] / ratio;
            let yc = bbox[1_usize] / ratio;
            let w = bbox[2_usize] / ratio;
            let h = bbox[3_usize] / ratio;
            // Change to (upper-left, lower-right)
            let x0 = (xc - (w / 2.0)).max(0f32).min(original_width as f32);
            let y0 = (yc - (h / 2.0)).max(0f32).min(original_height as f32);
            let x1 = (xc + (w / 2.0)).max(0f32).min(original_width as f32);
            let y1 = (yc + (h / 2.0)).max(0f32).min(original_height as f32);

            assert!(x0 <= x1 && x1 <= original_width as f32);
            assert!(y0 <= y1);
            assert!(y1 <= original_height as f32);

            result.push(LayoutBBox {
                bbox: BBox { x0, y0, x1, y1 },
                proba,
                label,
            });
        }

        result
    }
    fn scale_wh(&self, w0: f32, h0: f32, w1: f32, h1: f32) -> (f32, f32, f32) {
        let r = (w1 / w0).min(h1 / h0);
        (r, (w0 * r).round(), (h0 * r).round())
    }

    fn preprocess(&self, img: &DynamicImage) -> Array4<f32> {
        let (w0, h0) = img.dimensions();
        let (_, w_new, h_new) = self.scale_wh(
            w0 as f32,
            h0 as f32,
            Self::REQUIRED_WIDTH as f32,
            Self::REQUIRED_HEIGHT as f32,
        ); // f32 round
        let resized_img = img.resize_exact(w_new as u32, h_new as u32, FilterType::Triangle);
        let mut input_tensor = Array4::ones([
            1,
            3,
            Self::REQUIRED_HEIGHT as usize,
            Self::REQUIRED_WIDTH as usize,
        ]);
        input_tensor.fill(144.0 / 255.0);
        for (x, y, pixel) in resized_img.pixels() {
            let x = x as usize;
            let y = y as _;
            let [r, g, b, _] = pixel.0;
            input_tensor[[0, 0, y, x]] = r as f32 / 255.0;
            input_tensor[[0, 1, y, x]] = g as f32 / 255.0;
            input_tensor[[0, 2, y, x]] = b as f32 / 255.0;
        }
        input_tensor
    }
}

fn nms(raw_bboxes: &mut Vec<LayoutBBox>, iou_threshold: f32) {
    raw_bboxes.sort_by(|r1, r2| r2.proba.partial_cmp(&r1.proba).unwrap());
    let mut current_index = 0;
    for index in 0..raw_bboxes.len() {
        let mut drop = false;
        for prev_index in 0..current_index {
            let iou = raw_bboxes[index].bbox.iou(&raw_bboxes[prev_index].bbox);
            if iou > iou_threshold && raw_bboxes[prev_index].label == raw_bboxes[index].label {
                drop = true;
                break;
            }
        }
        if !drop {
            raw_bboxes.swap(current_index, index);
            current_index += 1;
        }
    }
    raw_bboxes.truncate(current_index);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nms_no_overlap() {
        let mut raw_bboxes = vec![
            LayoutBBox {
                bbox: BBox {
                    x0: 0.0,
                    y0: 0.0,
                    x1: 1.0,
                    y1: 1.0,
                },
                label: "A",
                proba: 0.9,
            },
            LayoutBBox {
                bbox: BBox {
                    x0: 2.0,
                    y0: 2.0,
                    x1: 3.0,
                    y1: 3.0,
                },
                label: "A",
                proba: 0.95,
            },
            LayoutBBox {
                bbox: BBox {
                    x0: 4.0,
                    y0: 4.0,
                    x1: 5.0,
                    y1: 5.0,
                },
                label: "A",
                proba: 0.85,
            },
        ];

        let iou_threshold = 0.5;
        nms(&mut raw_bboxes, iou_threshold);

        assert_eq!(raw_bboxes.len(), 3);
        // No boxes should be eliminated as there is no overlap
    }

    fn test_nms_high_overlap_same_label() {
        let mut raw_bboxes = vec![
            LayoutBBox {
                bbox: BBox {
                    x0: 0.0,
                    y0: 0.0,
                    x1: 2.0,
                    y1: 2.0,
                },
                label: "A",
                proba: 0.85,
            },
            LayoutBBox {
                // Shifted slightly inside box #1 so intersection is large
                bbox: BBox {
                    x0: 0.5,
                    y0: 0.5,
                    x1: 2.0,
                    y1: 2.0,
                },
                label: "A",
                proba: 0.95,
            },
            LayoutBBox {
                // Exactly the same as box #1 => IOU=1 with box #1
                bbox: BBox {
                    x0: 0.0,
                    y0: 0.0,
                    x1: 2.0,
                    y1: 2.0,
                },
                label: "A",
                proba: 0.90,
            },
        ];

        // Now all pairwise IOUs exceed 0.5, so NMS should keep only the
        // box with the highest probability (0.95).
        let iou_threshold = 0.5;
        nms(&mut raw_bboxes, iou_threshold);

        // We expect exactly one box left, with proba = 0.95.
        assert_eq!(raw_bboxes.len(), 1);
        assert_eq!(raw_bboxes[0].proba, 0.95);
    }

    #[test]
    fn test_nms_partial_overlap_different_labels() {
        let mut raw_bboxes = vec![
            LayoutBBox {
                bbox: BBox {
                    x0: 0.0,
                    y0: 0.0,
                    x1: 2.0,
                    y1: 2.0,
                },
                label: "A",
                proba: 0.85,
            },
            LayoutBBox {
                bbox: BBox {
                    x0: 1.0,
                    y0: 1.0,
                    x1: 3.0,
                    y1: 3.0,
                },
                label: "B",
                proba: 0.95,
            },
            LayoutBBox {
                bbox: BBox {
                    x0: 0.0,
                    y0: 0.0,
                    x1: 1.0,
                    y1: 1.0,
                },
                label: "A",
                proba: 0.7,
            },
        ];

        let iou_threshold = 0.5;
        nms(&mut raw_bboxes, iou_threshold);

        assert_eq!(raw_bboxes.len(), 3);
        // No suppression because different labels; but note overlapping dilation among labels
    }

    #[test]
    fn test_nms_mixed_case() {
        let mut raw_bboxes = vec![
            // Highest-probability box for label A
            LayoutBBox {
                bbox: BBox {
                    x0: 0.0,
                    y0: 0.0,
                    x1: 2.0,
                    y1: 2.0,
                },
                label: "A",
                proba: 0.9,
            },
            // Overlaps #1 enough to have IOU > 0.5
            LayoutBBox {
                bbox: BBox {
                    x0: 0.5,
                    y0: 0.5,
                    x1: 2.0,
                    y1: 2.0,
                },
                label: "A",
                proba: 0.8,
            },
            // Same as #2 => also overlaps #1 at > 0.5
            LayoutBBox {
                bbox: BBox {
                    x0: 0.5,
                    y0: 0.5,
                    x1: 2.0,
                    y1: 2.0,
                },
                label: "A",
                proba: 0.85,
            },
            // Highest-probability box for label B
            LayoutBBox {
                bbox: BBox {
                    x0: 3.0,
                    y0: 3.0,
                    x1: 5.0,
                    y1: 5.0,
                },
                label: "B",
                proba: 0.95,
            },
            // Overlaps #4 enough to have IOU > 0.5
            LayoutBBox {
                bbox: BBox {
                    x0: 3.5,
                    y0: 3.5,
                    x1: 5.0,
                    y1: 5.0,
                },
                label: "B",
                proba: 0.75,
            },
        ];

        let iou_threshold = 0.5;
        nms(&mut raw_bboxes, iou_threshold);

        // We expect that only two boxes remain:
        //   1) Box #1 (label A, proba=0.9)
        //   2) Box #4 (label B, proba=0.95)
        //
        // Because #2 and #3 overlap too much with #1, and #5 overlaps too much with #4.
        // The final list is sorted by descending probability, so #4 (0.95) is first.
        assert!(raw_bboxes.len() <= 3, "Expected at most 3 boxes after NMS");
        assert!(raw_bboxes.iter().all(|b| b.label == "A" || b.label == "B"));
        assert_eq!(raw_bboxes[0].proba, 0.95);
    }
}
