use std::path::Path;

use pdfium_render::prelude::{PdfPageTextChar, PdfRenderConfig, Pdfium};

use crate::{
    layout::model::{LayoutBBox, ORTLayoutParser},
    BBox, CharSpan, Line,
};

fn parse_text_spans<'a>(
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

fn parse_text_lines(spans: Vec<CharSpan>) -> Vec<Line> {
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
    let line_coverage_minimum: u32 = 1;
    let layout_coverage_threshold: f32 = 0.1;
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
        let text_spans = parse_text_spans(page.text()?.chars().iter(), &page_bbox);
        let text_lines = parse_text_lines(text_spans);

        // FIXME: check that rotation is correct ??
        // let page_rotation = page.rotation().unwrap_or(PdfPageRenderRotation::None);
        let page_image = page
            .render_with_config(&PdfRenderConfig::default().scale_page_by_factor(rescale_factor))
            .map(|bitmap| bitmap.as_image())?;
        //
        // TODO: Takes ~25ms -> batch a &[PdfPage] later
        // Export model with dynamic batch params
        let page_layout = layout_model.parse_layout(&page_image)?;

        let page_need_ocr = page_need_ocr(
            &text_lines,
            &page_layout,
            line_coverage_minimum,
            layout_coverage_threshold,
        );

        dbg!(page_need_ocr);

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

fn page_need_ocr(
    text_lines: &[Line],
    page_bboxes: &[LayoutBBox],
    line_coverage_minimum: u32,
    layout_coverage_threshold: f32,
) -> bool {
    let text_boxes: Vec<&LayoutBBox> = page_bboxes.iter().filter(|b| b.is_text_block()).collect();

    let coverage: u32 = text_lines
        .iter()
        .map(|l| {
            text_boxes.iter().fold(0, |acc, b| {
                acc + (b.bbox.intersection(&l.bbox) > 0f32) as u32
            })
        })
        .filter(|v| *v > line_coverage_minimum)
        .sum();

    // TODO: Model will sometimes say there is a single block of text on the page when it is blank
    // if not text_okay and (total_blocks == 1 and large_text_blocks == 1):
    //     text_okay = True
    //
    let text_line_coverage = if text_lines.is_empty() {
        (coverage / (text_lines.len() as u32)) as f32
    } else {
        layout_coverage_threshold + 1f32
    };

    text_line_coverage > layout_coverage_threshold
}
