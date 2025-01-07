use plsfix::fix_text;
use std::{
    path::{Path, PathBuf},
    time::Instant,
};

use pdfium_render::prelude::{
    PdfFontWeight, PdfPageRenderRotation, PdfPageTextChar, PdfRect, PdfRenderConfig, Pdfium,
};

use crate::{layout::model::ORTLayoutParser, BBox, CharSpan, Line};

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
    // let mut pages = Vec::with_capacity(document.pages().len() as usize);
    for (index, mut page) in document.pages_mut().iter().enumerate() {
        // TODO: deal with document embedded forms?
        if flatten_pdf {
            page.flatten()?;
        }
        let rescale_factor = {
            let scale_w = ORTLayoutParser::REQUIRED_WIDTH as f32 / page.width().value;
            let scale_h = ORTLayoutParser::REQUIRED_HEIGHT as f32 / page.height().value;
            f32::min(scale_h, scale_w)
        };

        let page_bbox = BBox {
            x0: 0f32,
            y0: 0f32,
            x1: page.width().value,
            y1: page.height().value,
        };
        let spans = parse_spans(page.text()?.chars().iter(), &page_bbox);
        let lines = parse_lines(spans);

        // FIXME: check that rotation is correct ??
        // let page_rotation = page.rotation().unwrap_or(PdfPageRenderRotation::None);
        let page_image = page
            .render_with_config(&PdfRenderConfig::default().scale_page_by_factor(rescale_factor))
            .map(|bitmap| bitmap.as_image())?;
        // TODO: Takes ~25ms -> batch a &[PdfPage] later
        // Export model with dynamic batch params
        layout_model.parse_layout(&page_image)?;

        if index >= 2 {
            break;
        }

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
