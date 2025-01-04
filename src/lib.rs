#![allow(dead_code)]

use layout::model::ORTLayoutParser;
use plsfix::fix_text;
use std::{
    path::{Path, PathBuf},
    time::Instant,
};

use pdfium_render::prelude::{
    PdfFontWeight, PdfPageRenderRotation, PdfPageTextChar, PdfRect, PdfRenderConfig, Pdfium,
};
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
fn parse_spans<'a>(
    chars: impl Iterator<Item = PdfPageTextChar<'a>>,
    page_bbox: &BBox,
) -> Vec<CharSpan> {
    let mut spans: Vec<CharSpan> = Vec::new();

    for char in chars {
        if spans.is_empty() {
            let span = CharSpan::new_from_char(&char, page_bbox);
            spans.push(span);
        } else {
            let span = spans.last_mut().unwrap();
            match span.append(&char, page_bbox) {
                Some(_) => {}
                None => {
                    let span = CharSpan::new_from_char(&char, page_bbox);
                    spans.push(span);
                }
            };
        }
    }

    spans
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

fn parse_lines(spans: Vec<CharSpan>) -> Vec<Line> {
    let mut lines = Vec::new();
    for span in spans {
        if lines.is_empty() {
            let line = Line::new_from_span(span);
            lines.push(line);
        } else {
            let line = lines.last_mut().unwrap();
            if let Err(span) = line.append(span) {
                let line = Line::new_from_span(span);
                lines.push(line)
            }
        }
    }

    lines
}

pub fn parse_document<P: AsRef<Path>>(
    path: P,
    password: Option<&str>,
    flatten_pdf: bool,
) -> anyhow::Result<()> {
    let pdfium = Pdfium::new(Pdfium::bind_to_statically_linked_library()?);
    let mut document = pdfium.load_pdf_from_file(&path, password)?;

    let layout_model = ORTLayoutParser::new("./models/yolov8s-doclaynet.onnx")?;
    // TODO: deal with document embedded forms?
    // let mut pages = Vec::with_capacity(document.pages().len() as usize);
    dbg!(&layout_model);
    for (index, mut page) in document.pages_mut().iter().enumerate() {
        if flatten_pdf {
            page.flatten()?;
        }
        // FIXME: check that rotation is correct ??
        // let page_rotation = page.rotation().unwrap_or(PdfPageRenderRotation::None);
        let page_image = page
            .render_with_config(&PdfRenderConfig::default().scale_page_by_factor(1.0))
            .map(|bitmap| bitmap.as_image())?;

        let page_bbox = BBox {
            x0: 0f32,
            y0: 0f32,
            x1: page.width().value,
            y1: page.height().value,
        };
        let spans = parse_spans(page.text()?.chars().iter(), &page_bbox);
        let lines = parse_lines(spans);

        // TODO: Takes ~25ms -> batch a &[PdfPage] later
        let layout_result = layout_model.parse_layout(&page_image)?;

        break;

        // pages.push(Page {
        //     id: index,
        //     blocks: vec![],
        //     width: page_bbox.bounds.width().value,
        //     height: page_bbox.bounds.height().value,
        //     rotation: page_rotation,
        // });
        //
    }

    Ok(())
}
