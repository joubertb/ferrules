use std::path::{Path, PathBuf};

use pdfium_render::prelude::{
    PdfPage, PdfPageRenderRotation, PdfPageTextChar, PdfRenderConfig, Pdfium,
};
use uuid::Uuid;

use crate::{
    entities::{BBox, Block, CharSpan, Document, Line, StructuredPage},
    layout::{
        draw::{draw_layout_bboxes, draw_ocr_bboxes, draw_text_lines},
        model::{LayoutBBox, ORTLayoutParser},
    },
    sanitize_doc_name,
};

#[cfg(target_os = "macos")]
use crate::ocr::parse_image_ocr;

const MIN_LAYOUT_COVERAGE_THRESHOLD: f32 = 0.2;

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

pub fn parse_page(
    page_idx: usize,
    page: &mut PdfPage,
    layout_model: &ORTLayoutParser,
    tmp_dir: &Path,
    flatten_pdf: bool,
) -> anyhow::Result<StructuredPage> {
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

    // FIXME: check that rotation is correct ?
    // Some page return 90 or 270 rotation but are correct...
    let page_rotation = page.rotation().unwrap_or(PdfPageRenderRotation::None);
    let page_image = page
        .render_with_config(&PdfRenderConfig::default().scale_page_by_factor(rescale_factor))
        .map(|bitmap| bitmap.as_image())?;

    // TODO(@aminediro): Takes ~25ms per page-> batch and send a [PdfPage; BATCH_SIZE]
    let page_layout = layout_model.parse_layout(&page_image, 1f32 / rescale_factor)?;

    let (need_ocr, blocks) = merge_line_layout(
        &page_layout,
        &text_lines,
        page_idx,
        MIN_LAYOUT_COVERAGE_THRESHOLD,
    )?;

    let ocr_result = if need_ocr {
        if cfg!(target_os = "macos") {
            let ocr_result = parse_image_ocr(&page_image, rescale_factor)?;
            Some(ocr_result)
        } else {
            println!("Target OS not macOS. Skipping OCR for now");
            None
        }
    } else {
        None
    };

    let structured_page = StructuredPage {
        id: page_idx,
        width: page_bbox.width(),
        height: page_bbox.height(),
        rotation: page_rotation,
        blocks,
        need_ocr,
    };

    if std::env::var("FERRULES_DEBUG").is_ok() {
        // TODO: add feature compile debug
        let output_file = tmp_dir.join(format!("page_{}.png", page_idx));
        let page_image = page
            .render_with_config(&PdfRenderConfig::default().scale_page_by_factor(1f32))
            .map(|bitmap| bitmap.as_image())?;
        let out_img = draw_text_lines(&text_lines, &page_image)?;
        let out_img = draw_layout_bboxes(&page_layout, &out_img.into())?;
        if let Some(ocr_result) = ocr_result {
            let out_img = draw_ocr_bboxes(&ocr_result, &out_img.into())?;
            out_img.save(output_file)?;
        } else {
            out_img.save(output_file)?;
        }
    };
    Ok(structured_page)
}

pub fn parse_document<P: AsRef<Path>>(
    path: P,
    layout_model: &ORTLayoutParser,
    password: Option<&str>,
    flatten_pdf: bool,
) -> anyhow::Result<Document<P>> {
    let doc_name = path
        .as_ref()
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&Uuid::new_v4().to_string())
        .to_string();

    let tmp_dir = PathBuf::from("/tmp").join(format!("ferrules-{}", sanitize_doc_name(&doc_name)));
    if std::env::var("FERRULES_DEBUG").is_ok() {
        std::fs::create_dir_all(&tmp_dir)?;
    }

    let pdfium = Pdfium::new(Pdfium::bind_to_statically_linked_library()?);
    let mut document = pdfium.load_pdf_from_file(&path, password)?;

    let pages: anyhow::Result<Vec<_>> = document
        .pages_mut()
        .iter()
        .enumerate()
        .map(|(page_idx, mut page)| {
            parse_page(page_idx, &mut page, layout_model, &tmp_dir, flatten_pdf)
        })
        .collect();

    if std::env::var("FERRULES_DEBUG").is_ok() {
        println!("Saved debug results in {:?}", tmp_dir.as_os_str());
    }

    Ok(Document {
        path,
        pages: pages?,
    })
}

fn merge_line_layout(
    page_layout: &[LayoutBBox],
    text_lines: &[Line],
    page_id: usize,
    layout_coverage_threshold: f32,
) -> anyhow::Result<(bool, Vec<Block>)> {
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
            if b.bbox.intersection(&line.bbox) > 0.5 {
                total_line_coverage += 1;
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
                // TODO : "Line skipped because no layout bbox intersection");
                continue;
            }
        }
    }

    // let need_ocr = if !text_lines.is_empty() {
    //     let text_line_coverage = total_line_coverage as f32 / text_lines.len() as f32;

    //     let matched_blocks: Vec<_> = blocks.iter().map(|b| b.layout_block_id).collect();

    //     let layout_text_no_lines = text_boxes
    //         .iter()
    //         .filter(|layout_bbox| !matched_blocks.contains(&layout_bbox.id))
    //         .count();
    //     (text_line_coverage) < layout_coverage_threshold
    //         || (layout_text_no_lines as f32 / text_boxes.len() as f32) > 0.8
    // } else {
    //     true
    // };

    let line_area = text_lines.iter().map(|l| l.bbox.area()).sum::<f32>();
    let text_layoutbbox_area = text_boxes.iter().map(|l| l.bbox.area()).sum::<f32>();

    let need_ocr = if text_layoutbbox_area > 0f32 {
        line_area / text_layoutbbox_area < 0.5
    } else {
        true
    };

    Ok((need_ocr, blocks))
}
