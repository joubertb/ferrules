#![allow(dead_code)]

use plsfix::fix_text;
use std::path::PathBuf;
pub mod parse;

use pdfium_render::prelude::{PdfFontWeight, PdfPageRenderRotation, PdfPageTextChar, PdfRect};
pub mod layout;

#[derive(Debug, Default, Clone)]
struct BBox {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

impl BBox {
    fn from_pdfrect(
        PdfRect {
            bottom,
            left,
            top,
            right,
        }: PdfRect,
        page_height: f32,
    ) -> Self {
        Self {
            x0: left.value,
            y0: page_height - top.value,
            x1: right.value,
            y1: page_height - bottom.value,
        }
    }
    fn height(&self) -> f32 {
        self.y1 - self.y0
    }
    fn width(&self) -> f32 {
        self.x1 - self.x0
    }
    fn area(&self) -> f32 {
        self.height() * self.width()
    }
    fn size(&self) -> (f32, f32) {
        (self.width(), self.height())
    }

    fn merge(&mut self, other: &Self) -> Self {
        let x0 = self.x0.min(other.x0);
        let y0 = self.y0.min(other.y0);
        let x1 = self.x1.max(other.x1);
        let y1 = self.y1.max(other.y1);
        Self { x0, y0, x1, y1 }
    }
    fn overlap_x(&self, other: &Self) -> f32 {
        f32::max(
            0f32,
            f32::min(self.x1, other.x1) - f32::max(self.x0, other.x0),
        )
    }
    fn overlap_y(&self, other: &Self) -> f32 {
        f32::max(
            0f32,
            f32::min(self.y1, other.y1) - f32::max(self.y0, other.y0),
        )
    }

    fn iou(&self, other: &Self) -> f32 {
        self.intersection(other) / self.union(other)
    }

    fn intersection(&self, other: &Self) -> f32 {
        self.overlap_x(other) * self.overlap_y(other)
    }

    fn union(&self, other: &Self) -> f32 {
        other.area() + self.area() - self.intersection(other)
    }

    fn rotate(self) -> Self {
        todo!()
    }
}

#[derive(Debug)]
enum BlockType {
    Header,
    Footer,
    Text,
    Line,
    Span,
    Image,
}

#[derive(Debug)]
struct Block {
    id: usize,
    kind: BlockType,
    page_id: usize,
    bbox: BBox,
}

#[derive(Debug)]
struct Page {
    id: usize,
    blocks: Vec<Block>,
    width: f32,
    height: f32,
    rotation: PdfPageRenderRotation,
}

#[derive(Debug)]
struct Document {
    path: PathBuf,
    pages: Vec<Page>,
}

#[derive(Debug)]
struct CharSpan {
    bbox: BBox,
    text: String,
    rotation: f32,
    font_name: String,
    font_size: f32,
    font_weight: Option<PdfFontWeight>,
    char_start_idx: usize,
    char_end_idx: usize,
}

impl CharSpan {
    pub fn new_from_char(char: &PdfPageTextChar, page_bbox: &BBox) -> Self {
        Self {
            bbox: BBox::from_pdfrect(
                char.tight_bounds()
                    .expect("Error init span tight bound char"),
                page_bbox.width(),
            ),
            text: char.unicode_char().unwrap_or_default().into(),
            font_name: char.font_name(),
            font_weight: char.font_weight(),
            font_size: char.unscaled_font_size().value,
            rotation: char.get_rotation_clockwise_degrees(),
            char_start_idx: char.index(),
            char_end_idx: char.index(),
        }
    }
    pub fn append(&mut self, char: &PdfPageTextChar, page_bbox: &BBox) -> Option<()> {
        let char_rotation = char.get_rotation_clockwise_degrees();
        if char.unscaled_font_size().value != self.font_size
            || char.font_name() != self.font_name
            || char.font_weight() != self.font_weight
            || char_rotation != self.rotation
        {
            None
        } else {
            let char_bbox = BBox::from_pdfrect(
                char.tight_bounds().expect("error tight bound"),
                page_bbox.height(),
            );
            self.text.push(char.unicode_char().unwrap_or_default());
            self.char_end_idx = char.index();
            self.bbox.merge(&char_bbox);
            Some(())
        }
    }
}
#[derive(Debug, Default)]
struct Line {
    bbox: BBox,
    text: String,
    rotation: f32,
    spans: Vec<CharSpan>,
}

impl Line {
    fn new_from_span(span: CharSpan) -> Self {
        Self {
            bbox: span.bbox.clone(),
            text: span.text.clone(),
            rotation: span.rotation,
            spans: vec![span],
        }
    }
    // TODO: find a better pattern here
    // return Some if we fail to append the span-> not great
    fn append(&mut self, span: CharSpan) -> Result<(), CharSpan> {
        if span.rotation != self.rotation
        // NOTE: sometimes pdfium doesn't inject a linebreak, so we check the span positions
        || span.bbox.y0 > self.bbox.y1
        || span.text.ends_with("\n") || span.text.ends_with("\x02")
        {
            self.text = fix_text(&self.text, None);
            Err(span)
        } else {
            self.bbox.merge(&span.bbox);
            self.text.push_str(&span.text);
            self.spans.push(span);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intersection() {
        let bbox1 = BBox {
            x0: 0.0,
            y0: 0.0,
            x1: 2.0,
            y1: 2.0,
        };
        let bbox2 = BBox {
            x0: 1.0,
            y0: 1.0,
            x1: 3.0,
            y1: 3.0,
        };
        let bbox3 = BBox {
            x0: 2.0,
            y0: 2.0,
            x1: 4.0,
            y1: 4.0,
        };
        let bbox4 = BBox {
            x0: 3.0,
            y0: 3.0,
            x1: 5.0,
            y1: 5.0,
        }; // No overlap
        let bbox5 = BBox {
            x0: -1.0,
            y0: -1.0,
            x1: 1.0,
            y1: 1.0,
        }; // Negative coordinates
        let bbox6 = BBox {
            x0: 0.5,
            y0: 0.5,
            x1: 1.5,
            y1: 1.5,
        }; // Inside bbox1

        // Edge Cases
        assert_eq!(bbox1.intersection(&bbox3), 0.0);
        assert_eq!(bbox1.intersection(&bbox4), 0.0); // Adjacent
        assert_eq!(bbox5.intersection(&bbox1), 1.0); // Overlaps partially with bbox1

        // Overlaps
        assert_eq!(bbox1.intersection(&bbox2), 1.0);
        assert_eq!(bbox1.intersection(&bbox6), bbox6.area()); // bbox6 is inside bbox1

        // Sanity Checks
        assert_eq!(bbox1.intersection(&bbox1), bbox1.area());
    }

    #[test]
    fn test_union() {
        let bbox1 = BBox {
            x0: 0.0,
            y0: 0.0,
            x1: 2.0,
            y1: 2.0,
        };
        let bbox2 = BBox {
            x0: 1.0,
            y0: 1.0,
            x1: 3.0,
            y1: 3.0,
        };
        let bbox3 = BBox {
            x0: 2.0,
            y0: 2.0,
            x1: 4.0,
            y1: 4.0,
        };
        let bbox4 = BBox {
            x0: 3.0,
            y0: 3.0,
            x1: 5.0,
            y1: 5.0,
        }; // No overlap
        let bbox5 = BBox {
            x0: -1.0,
            y0: -1.0,
            x1: 1.0,
            y1: 1.0,
        }; // Negative coordinates

        // Edge Cases
        assert_eq!(bbox1.union(&bbox3), 8.0);
        assert_eq!(bbox1.union(&bbox4), 8.0); // Completely non-overlapping
        assert_eq!(bbox5.union(&bbox1), 7.0); // Negative coordinate case

        // Overlapping
        assert_eq!(bbox1.union(&bbox2), 7.0);

        // Sanity Checks
        assert_eq!(bbox1.union(&bbox1), bbox1.area());
    }

    #[test]
    fn test_iou() {
        let bbox1 = BBox {
            x0: 0.0,
            y0: 0.0,
            x1: 2.0,
            y1: 2.0,
        };
        let bbox2 = BBox {
            x0: 1.0,
            y0: 1.0,
            x1: 3.0,
            y1: 3.0,
        };
        let bbox3 = BBox {
            x0: 2.0,
            y0: 2.0,
            x1: 4.0,
            y1: 4.0,
        };
        let bbox4 = BBox {
            x0: 3.0,
            y0: 3.0,
            x1: 5.0,
            y1: 5.0,
        }; // No overlap
        let bbox6 = BBox {
            x0: 0.5,
            y0: 0.5,
            x1: 1.5,
            y1: 1.5,
        }; // Inside bbox1

        // Sanity Checks
        assert_eq!(bbox1.iou(&bbox1), 1.0);
        // Completely non-overlapping
        assert_eq!(bbox1.iou(&bbox4), 0.0);

        // Edge Cases
        assert_eq!(bbox1.iou(&bbox3), 0.0);

        // Overlapping
        assert_eq!(bbox1.iou(&bbox2), 1.0 / 7.0);
        assert_eq!(bbox1.iou(&bbox6), bbox6.area() / bbox1.area()); // bbox6 is inside bbox1
    }
}
