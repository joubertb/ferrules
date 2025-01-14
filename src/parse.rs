use std::{
    fmt::Write,
    path::{Path, PathBuf},
    time::Instant,
};

use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use itertools::izip;
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

/// This constant defines the minimum ratio between the area of text lines identified
/// by the pdfium2 and the area of text regions detected through layout analysis.
/// If this ratio falls below the threshold of 0.5 (or 50%), it indicates that the page
/// may not have enough __native__ lines, and therefore should
/// be considered for OCR to ensure accurate text extraction.
const MIN_LAYOUT_COVERAGE_THRESHOLD: f32 = 0.5;

/// This constant defines the minimum required intersection ratio between the bounding box of an
/// OCR-detected text line and a text block detected through layout analysis.
/// This approach ensures that only text lines significantly overlapping with a layout block are
/// paired, thus improving the accuracy of OCR-text and layout alignment.
const MIN_INTERSECTION_LAYOUT: f32 = 0.5;

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
    let rescale_factor = {
        let scale_w = ORTLayoutParser::REQUIRED_WIDTH as f32 / page.width().value;
        let scale_h = ORTLayoutParser::REQUIRED_HEIGHT as f32 / page.height().value;
        f32::min(scale_h, scale_w)
    };
    let page_rotation = page.rotation().unwrap_or(PdfPageRenderRotation::None);
    let page_image = page
        .render_with_config(&PdfRenderConfig::default().scale_page_by_factor(rescale_factor))
        .map(|bitmap| bitmap.as_image())?;

    // TODO(@aminediro): Takes ~25ms per page-> batch and send a [PdfPage; BATCH_SIZE]
    let page_layout = layout_model.parse_layout(&page_image, 1f32 / rescale_factor)?;

    let text_boxes: Vec<&LayoutBBox> = page_layout.iter().filter(|b| b.is_text_block()).collect();

    let need_ocr = page_needs_ocr(&text_boxes, &text_lines);

    let ocr_result = if need_ocr {
        if cfg!(target_os = "macos") {
            let ocr_result = parse_image_ocr(&page_image, rescale_factor)?;
            Some(ocr_result)
        } else {
            None
        }
    } else {
        None
    };

    let blocks = if need_ocr && ocr_result.is_some() {
        let lines = ocr_result
            .as_ref()
            .unwrap()
            .iter()
            .map(|ocr_line| ocr_line.to_line())
            .collect::<Vec<_>>();
        merge_lines_layout(&text_boxes, &lines, page_idx)?
    } else {
        merge_lines_layout(&text_boxes, &text_lines, page_idx)?
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

pub fn parse_pages(
    pages: &mut [(usize, PdfPage)],
    layout_model: &ORTLayoutParser,
    tmp_dir: &Path,
    flatten_pdf: bool,
    debug: bool,
    pb: &ProgressBar,
) -> anyhow::Result<Vec<StructuredPage>> {
    // TODO: deal with document embedded forms?
    for (_, page) in pages.iter_mut() {
        if flatten_pdf {
            page.flatten()?;
        }
    }
    let rescale_factors: Vec<f32> = pages
        .iter()
        .map(|(_, page)| {
            let scale_w = ORTLayoutParser::REQUIRED_WIDTH as f32 / page.width().value;
            let scale_h = ORTLayoutParser::REQUIRED_HEIGHT as f32 / page.height().value;
            f32::min(scale_h, scale_w)
        })
        .collect();

    let page_images: Result<Vec<_>, _> = pages
        .iter()
        .zip(rescale_factors.iter())
        .map(|((_, page), rescale_factor)| {
            page.render_with_config(
                &PdfRenderConfig::default().scale_page_by_factor(*rescale_factor),
            )
            .map(|bitmap| bitmap.as_image())
        })
        .collect();
    let page_images = page_images?;

    let downscale_factors = rescale_factors
        .iter()
        .map(|f| 1f32 / *f)
        .collect::<Vec<f32>>();

    let pages_layout = layout_model.parse_layout_batch(&page_images, &downscale_factors)?;

    let mut structured_pages = Vec::with_capacity(pages.len());

    for ((page_idx, page), page_layout, page_image, downscale_factor) in izip![
        pages.iter(),
        &pages_layout,
        &page_images,
        &downscale_factors
    ] {
        let page_bbox = BBox {
            x0: 0f32,
            y0: 0f32,
            x1: page.width().value,
            y1: page.height().value,
        };

        let page_rotation = page.rotation().unwrap_or(PdfPageRenderRotation::None);
        let text_spans = parse_text_spans(page.text()?.chars().iter(), &page_bbox);
        let text_lines = parse_text_lines(text_spans);

        let text_boxes: Vec<&LayoutBBox> =
            page_layout.iter().filter(|b| b.is_text_block()).collect();
        let need_ocr = page_needs_ocr(&text_boxes, &text_lines);

        let ocr_result = if need_ocr {
            if cfg!(target_os = "macos") {
                let ocr_result = parse_image_ocr(page_image, *downscale_factor)?;
                Some(ocr_result)
            } else {
                None
            }
        } else {
            None
        };

        let blocks = if need_ocr && ocr_result.is_some() {
            let lines = ocr_result
                .as_ref()
                .unwrap()
                .iter()
                .map(|ocr_line| ocr_line.to_line())
                .collect::<Vec<_>>();
            merge_lines_layout(&text_boxes, &lines, *page_idx)?
        } else {
            merge_lines_layout(&text_boxes, &text_lines, *page_idx)?
        };

        let structured_page = StructuredPage {
            id: *page_idx,
            width: page_bbox.width(),
            height: page_bbox.height(),
            rotation: page_rotation,
            blocks,
            need_ocr,
        };

        structured_pages.push(structured_page);

        pb.set_message(format!("item #{}", *page_idx + 1));
        pb.inc(1u64);
        if debug {
            // TODO: add feature compile debug
            let output_file = tmp_dir.join(format!("page_{}.png", page_idx));
            let page_image = page
                .render_with_config(&PdfRenderConfig::default().scale_page_by_factor(1f32))
                .map(|bitmap| bitmap.as_image())?;
            let out_img = draw_text_lines(&text_lines, &page_image)?;
            let out_img = draw_layout_bboxes(page_layout, &out_img.into())?;
            if let Some(ocr_result) = ocr_result {
                let out_img = draw_ocr_bboxes(&ocr_result, &out_img.into())?;
                out_img.save(output_file)?;
            } else {
                out_img.save(output_file)?;
            }
        };
    }

    Ok(structured_pages)
}

pub fn parse_document<P: AsRef<Path>>(
    path: P,
    layout_model: &ORTLayoutParser,
    password: Option<&str>,
    flatten_pdf: bool,
    debug: bool,
) -> anyhow::Result<Document<P>> {
    let start_time = Instant::now();
    let doc_name = path
        .as_ref()
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&Uuid::new_v4().to_string())
        .to_string();

    let tmp_dir = PathBuf::from("/tmp").join(format!("ferrules-{}", sanitize_doc_name(&doc_name)));
    if debug {
        std::fs::create_dir_all(&tmp_dir)?;
    }

    let pdfium = Pdfium::new(Pdfium::bind_to_statically_linked_library()?);
    let mut document = pdfium.load_pdf_from_file(&path, password)?;

    let mut pages: Vec<_> = document.pages_mut().iter().enumerate().collect();
    let chunk_size = 32;

    let pb = ProgressBar::new(pages.len() as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {msg}",
        )
        .unwrap()
        .with_key("eta", |state: &ProgressState, w: &mut dyn Write| {
            write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap()
        })
        .progress_chars("#>-"),
    );

    let pages = pages
        .chunks_mut(chunk_size)
        .flat_map(|chunk| parse_pages(chunk, layout_model, &tmp_dir, flatten_pdf, debug, &pb))
        .flatten()
        .collect::<Vec<_>>();

    if debug {
        println!("Saved debug results in {:?}", tmp_dir.as_os_str());
    }

    let duration = Instant::now().duration_since(start_time).as_millis();
    pb.finish_with_message(format!("Parsed document in {}ms", duration));

    Ok(Document { path, pages })
}

fn page_needs_ocr(text_boxes: &[&LayoutBBox], text_lines: &[Line]) -> bool {
    let line_area = text_lines.iter().map(|l| l.bbox.area()).sum::<f32>();
    let text_layoutbbox_area = text_boxes.iter().map(|l| l.bbox.area()).sum::<f32>();

    if text_layoutbbox_area > 0f32 {
        line_area / text_layoutbbox_area < MIN_LAYOUT_COVERAGE_THRESHOLD
    } else {
        true
    }
}

/// Merges lines into blocks based on their layout, maintaining the order of lines.
///
/// This function takes a list of text boxes representing layout bounding boxes that contain text,
/// and a list of lines (which could be obtained from OCR or  PDF library pdfium2,
/// and merges these lines into blocks. The merging is done based on the intersection
/// of each line with the layout bounding boxes. The function prioritizes maintaining
/// the order of the lines, rather than the layout blocks.
fn merge_lines_layout(
    text_boxes: &[&LayoutBBox],
    lines: &[Line],
    page_id: usize,
) -> anyhow::Result<Vec<Block>> {
    let line_block_iterator = lines.iter().map(|line| {
        // Get max intersection block for the line
        let max_intersection_bbox = text_boxes.iter().max_by(|a, b| {
            let a_intersection = a.bbox.intersection(&line.bbox);
            let b_intersection = b.bbox.intersection(&line.bbox);

            a_intersection.partial_cmp(&b_intersection).unwrap()
        });
        let max_intersection_bbox = max_intersection_bbox.and_then(|b| {
            if b.bbox.intersection(&line.bbox) > MIN_INTERSECTION_LAYOUT {
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
                continue;
            }
        }
    }

    Ok(blocks)
}
