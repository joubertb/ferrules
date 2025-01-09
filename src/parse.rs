use std::path::Path;

use pdfium_render::prelude::{PdfPageTextChar, PdfRenderConfig, Pdfium};

use crate::{
    layout::{
        draw::{draw_bboxes, draw_text_lines},
        model::{LayoutBBox, ORTLayoutParser},
    },
    BBox, Block, CharSpan, Line,
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
    let layout_coverage_threshold: f32 = 0.1;
    let pdfium = Pdfium::new(Pdfium::bind_to_statically_linked_library()?);
    let mut document = pdfium.load_pdf_from_file(&path, password)?;

    let layout_model = ORTLayoutParser::new("./models/yolov8s-doclaynet.onnx")?;
    // let mut pages = Vec::with_capacity(document.pages().len() as usize);
    for (page_idx, mut page) in document.pages_mut().iter().enumerate() {
        // TODO: deal with document embedded forms?
        if flatten_pdf {
            page.flatten()?;
        }
        let rescale_factor = {
            let scale_w = ORTLayoutParser::REQUIRED_WIDTH as f32 / page.width().value;
            let scale_h = ORTLayoutParser::REQUIRED_HEIGHT as f32 / page.height().value;
            f32::min(scale_h, scale_w)
        };
        // TODO: change
        // let rescale_factor = 1f32;
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

        // TODO: Takes ~25ms -> batch a &[PdfPage] later
        // Export model with dynamic batch params
        let page_layout = layout_model.parse_layout(&page_image, 1f32 / rescale_factor)?;

        if std::env::var("FERRULES_DEBUG").is_ok() {
            let output_file = "page_line.png";
            let page_image = page
                .render_with_config(&PdfRenderConfig::default().scale_page_by_factor(1f32))
                .map(|bitmap| bitmap.as_image())?;
            let out_img = draw_text_lines(&text_lines, &page_image)?;
            let out_img = draw_bboxes(&page_layout, &out_img.into())?;
            out_img.save(output_file)?;
        };

        // let page_need_ocr = page_need_ocr(
        //     &text_lines,
        //     &page_layout,
        //     // line_coverage_minimum,
        //     layout_coverage_threshold,
        // );

        let blocks = merge_line_layout(&page_layout, &text_lines, page_idx);

        if page_idx >= 0 {
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

fn merge_line_layout(
    page_layout: &[LayoutBBox],
    text_lines: &[Line],
    page_id: usize,
) -> anyhow::Result<Vec<Block>> {
    let text_boxes: Vec<&LayoutBBox> = page_layout.iter().filter(|b| b.is_text_block()).collect();

    let mut total_line_coverage = 0;

    let line_block_iterator = text_lines.iter().map(|line| {
        // Get max intersection block for the line
        let max_intersection_bbox = text_boxes.iter().max_by(|a, b| {
            let a_intersection = a.bbox.intersection(&line.bbox);
            let b_intersection = b.bbox.intersection(&line.bbox);

            a_intersection.partial_cmp(&b_intersection).unwrap()
        });
        let max_intersection_bbox = max_intersection_bbox.and_then(|b| {
            if b.bbox.intersection(&line.bbox) > 0.8 {
                Some(b)
            } else {
                None
            }
        });
        (line, max_intersection_bbox)
    });

    // TODO: Return the page intersection
    let mut blocks = Vec::new();
    for (line, layout_block) in line_block_iterator {
        match layout_block {
            Some(&layoutb) => {
                if blocks.is_empty() {
                    let mut block = Block::from_layout_block(0, layoutb, page_id);
                    block.push_line(line);
                    blocks.push(block);
                }

                let last_block = blocks.last_mut().unwrap();

                if layoutb.id == last_block.layout_block_id {
                    last_block.push_line(line);
                } else {
                    let mut block = Block::from_layout_block(blocks.len() + 1, layoutb, page_id);
                    block.push_line(line);
                    blocks.push(block);
                }
            }
            None => {
                // TODO: add box
                // eprintln!("Line skipped because no layout bbox intersection");
                continue;
            }
        }
    }

    dbg!(&blocks);

    Ok(blocks)
}

fn page_need_ocr(
    text_lines: &[Line],
    page_bboxes: &[LayoutBBox],
    layout_coverage_threshold: f32,
) -> bool {
    let text_boxes: Vec<&LayoutBBox> = page_bboxes.iter().filter(|b| b.is_text_block()).collect();

    let mut total_line_coverage = 0;

    for line in text_lines {
        let max_intersection_bbox = text_boxes.iter().max_by(|a, b| {
            let a_intersection = a.bbox.intersection(&line.bbox);
            let b_intersection = b.bbox.intersection(&line.bbox);

            a_intersection.partial_cmp(&b_intersection).unwrap()
        });

        if let Some(layout_bbox) = max_intersection_bbox {
            total_line_coverage += (layout_bbox.bbox.intersection(&line.bbox) > 0f32) as usize;
        }
    }

    if !text_lines.is_empty() {
        let text_line_coverage = (total_line_coverage / text_lines.len()) as f32;
        text_line_coverage > layout_coverage_threshold
    } else {
        true
    }
}
