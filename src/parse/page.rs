use std::{path::Path, sync::Arc, time::Instant};

use anyhow::Context;
use image::DynamicImage;
use indicatif::ProgressBar;
use itertools::izip;
use pdfium_render::prelude::{PdfPage, PdfRenderConfig};
use tokio::sync::oneshot;

use crate::{
    draw::{draw_blocks, draw_layout_bboxes, draw_text_lines},
    entities::{BBox, Element, Line, PageID, StructuredPage},
    layout::{
        model::{LayoutBBox, Metadata, ORTLayoutParser, ParseLayoutRequest},
        ParseLayoutQueue,
    },
    ocr::parse_image_ocr,
};

use super::{
    merge::{merge_elements_into_blocks, merge_lines_layout, merge_remaining},
    native::{parse_text_lines, parse_text_spans},
};

/// This constant defines the minimum ratio between the area of text lines identified
/// by the pdfium2 and the area of text regions detected through layout analysis.
/// If this ratio falls below the threshold of 0.5 (or 50%), it indicates that the page
/// may not have enough __native__ lines, and therefore should
/// be considered for OCR to ensure accurate text extraction.
const MIN_LAYOUT_COVERAGE_THRESHOLD: f32 = 0.5;

fn page_needs_ocr(text_boxes: &[&LayoutBBox], text_lines: &[Line]) -> bool {
    let line_area = text_lines.iter().map(|l| l.bbox.area()).sum::<f32>();
    let text_layoutbbox_area = text_boxes.iter().map(|l| l.bbox.area()).sum::<f32>();

    if text_layoutbbox_area > 0f32 {
        line_area / text_layoutbbox_area < MIN_LAYOUT_COVERAGE_THRESHOLD
    } else {
        true
    }
}

fn build_page_elements(
    page_layout: &Vec<LayoutBBox>,
    text_lines: &[Line],
    page_idx: PageID,
) -> anyhow::Result<Vec<Element>> {
    let mut elements = merge_lines_layout(page_layout, text_lines, page_idx)?;
    let merged_layout_blocks_ids = elements
        .iter()
        .map(|e| e.layout_block_id)
        .collect::<Vec<_>>();
    let unmerged_layout_boxes: Vec<&LayoutBBox> = page_layout
        .iter()
        .filter(|&b| !merged_layout_blocks_ids.contains(&b.id))
        .collect();

    merge_remaining(&mut elements, &unmerged_layout_boxes, page_idx);
    Ok(elements)
}

fn parse_page_text(
    page: &PdfPage,
    page_layout: &[LayoutBBox],
    page_image: &DynamicImage,
    page_bbox: &BBox,
    downscale_factor: f32,
) -> anyhow::Result<(Vec<Line>, bool)> {
    // let page_rotation = page.rotation().unwrap_or(PdfPageRenderRotation::None);
    let text_spans = parse_text_spans(page.text()?.chars().iter(), page_bbox);
    let text_lines = parse_text_lines(text_spans);

    let text_layout_box: Vec<&LayoutBBox> =
        page_layout.iter().filter(|b| b.is_text_block()).collect();
    let need_ocr = page_needs_ocr(&text_layout_box, &text_lines);

    let ocr_result = if need_ocr {
        parse_image_ocr(page_image, downscale_factor).ok()
    } else {
        None
    };

    let lines = if need_ocr && ocr_result.is_some() {
        let lines = ocr_result
            .as_ref()
            .unwrap()
            .iter()
            .map(|ocr_line| ocr_line.to_line())
            .collect::<Vec<_>>();
        lines
    } else {
        text_lines
    };
    Ok((lines, need_ocr))
}

pub fn parse_pages(
    pdf_pages: &mut [(PageID, PdfPage)],
    layout_model: &ORTLayoutParser,
    tmp_dir: &Path,
    flatten_pdf: bool,
    debug: bool,
    pb: &ProgressBar,
) -> anyhow::Result<Vec<StructuredPage>> {
    // TODO: deal with document embedded forms?
    for (_, page) in pdf_pages.iter_mut() {
        if flatten_pdf {
            page.flatten()?;
        }
    }
    let rescale_factors: Vec<f32> = pdf_pages
        .iter()
        .map(|(_, page)| {
            let scale_w = ORTLayoutParser::REQUIRED_WIDTH as f32 / page.width().value;
            let scale_h = ORTLayoutParser::REQUIRED_HEIGHT as f32 / page.height().value;
            f32::min(scale_h, scale_w)
        })
        .collect();

    let page_images: Result<Vec<_>, _> = pdf_pages
        .iter()
        .zip(rescale_factors.iter())
        .map(|((_, page), rescale_factor)| {
            page.render_with_config(
                &PdfRenderConfig::default().scale_page_by_factor(*rescale_factor),
            )
            .map(|bitmap| bitmap.as_image())
        })
        .collect();

    let page_images = page_images.context("error rasterizing pages to images")?;

    let downscale_factors = rescale_factors
        .iter()
        .map(|f| 1f32 / *f)
        .collect::<Vec<f32>>();

    let pages_layout = layout_model.parse_layout_batch(&page_images, &downscale_factors)?;

    let mut structured_pages = Vec::with_capacity(pdf_pages.len());

    for ((page_idx, page), page_layout, page_image, downscale_factor) in izip![
        pdf_pages.iter(),
        &pages_layout,
        page_images,
        &downscale_factors
    ] {
        let page_bbox = BBox {
            x0: 0f32,
            y0: 0f32,
            x1: page.width().value,
            y1: page.height().value,
        };

        // let page_rotation = page.rotation().unwrap_or(PdfPageRenderRotation::None);
        let (text_lines, need_ocr) = parse_page_text(
            page,
            page_layout,
            &page_image,
            &page_bbox,
            *downscale_factor,
        )?;

        // Merging elements with layout
        let elements = build_page_elements(page_layout, &text_lines, *page_idx)?;

        // Rerender page image
        let page_image = page
            .render_with_config(&PdfRenderConfig::default().scale_page_by_factor(1f32))
            .map(|bitmap| bitmap.as_image())?;

        if debug {
            // TODO: add feature compile debug
            let output_file = tmp_dir.join(format!("page_{}.png", page_idx));
            let final_output_file = tmp_dir.join(format!("page_blocks_{}.png", page_idx));
            let out_img = draw_text_lines(&text_lines, &page_image, need_ocr)?;
            let out_img = draw_layout_bboxes(page_layout, &out_img.into())?;
            // Draw the final prediction -
            let blocks = merge_elements_into_blocks(elements.clone())?;
            let final_img = draw_blocks(&blocks, &page_image)?;
            out_img.save(output_file)?;

            final_img.save(final_output_file)?;
        };

        let structured_page = StructuredPage {
            id: *page_idx,
            width: page_bbox.width(),
            height: page_bbox.height(),
            image: page_image,
            elements,
            need_ocr,
        };

        structured_pages.push(structured_page);

        // TODO should be a callback
        pb.set_message(format!("Page #{}", *page_idx + 1));
        pb.inc(1u64);
    }

    Ok(structured_pages)
}

pub async fn parse_page_async<'a, F>(
    page_idx: PageID,
    page: &mut PdfPage<'a>,
    tmp_dir: &Path,
    flatten_pdf: bool,
    layout_queue: ParseLayoutQueue,
    debug: bool,
    callback: F,
) -> anyhow::Result<StructuredPage>
where
    F: FnOnce(&StructuredPage),
{
    // TODO: deal with document embedded forms?
    if flatten_pdf {
        page.flatten()?;
    }
    let rescale_factor = {
        let scale_w = ORTLayoutParser::REQUIRED_WIDTH as f32 / page.width().value;
        let scale_h = ORTLayoutParser::REQUIRED_HEIGHT as f32 / page.height().value;
        f32::min(scale_h, scale_w)
    };
    let downscale_factor = 1f32 / rescale_factor;

    let page_image = Arc::new(
        page.render_with_config(&PdfRenderConfig::default().scale_page_by_factor(rescale_factor))
            .map(|bitmap| bitmap.as_image())?,
    );

    let (layout_tx, layout_rx) = oneshot::channel();
    let layout_req = ParseLayoutRequest {
        page_id: page_idx,
        page_image: Arc::clone(&page_image),
        downscale_factor,
        metadata: Metadata {
            response_tx: layout_tx,
            queue_time: Instant::now(),
        },
    };
    layout_queue.push(layout_req).await?;

    let page_layout = layout_rx
        .await
        .context("error receiving layout on channel")?
        .context("error parsing page")?;

    let page_bbox = BBox {
        x0: 0f32,
        y0: 0f32,
        x1: page.width().value,
        y1: page.height().value,
    };

    // let page_rotation = page.rotation().unwrap_or(PdfPageRenderRotation::None);
    let (text_lines, need_ocr) = parse_page_text(
        page,
        &page_layout,
        &page_image,
        &page_bbox,
        downscale_factor,
    )?;

    // Merging elements with layout
    let elements = build_page_elements(&page_layout, &text_lines, page_idx)?;

    // Rerender page image
    let page_image = page
        .render_with_config(&PdfRenderConfig::default().scale_page_by_factor(1f32))
        .map(|bitmap| bitmap.as_image())?;

    if debug {
        debug_page(
            tmp_dir,
            page_idx,
            &page_image,
            &text_lines,
            need_ocr,
            &page_layout,
            &elements,
        )?
    };

    let structured_page = StructuredPage {
        id: page_idx,
        width: page_bbox.width(),
        height: page_bbox.height(),
        image: page_image,
        elements,
        need_ocr,
    };

    callback(&structured_page);

    Ok(structured_page)
}

fn debug_page(
    tmp_dir: &Path,
    page_idx: PageID,
    page_image: &DynamicImage,
    text_lines: &[Line],
    need_ocr: bool,
    page_layout: &[LayoutBBox],
    elements: &[Element],
) -> anyhow::Result<()> {
    let output_file = tmp_dir.join(format!("page_{}.png", page_idx));
    let final_output_file = tmp_dir.join(format!("page_blocks_{}.png", page_idx));
    let out_img = draw_text_lines(text_lines, page_image, need_ocr)?;
    let out_img = draw_layout_bboxes(page_layout, &out_img.into())?;
    // Draw the final prediction -
    let blocks = merge_elements_into_blocks(elements.to_vec())?;
    let final_img = draw_blocks(&blocks, page_image)?;
    out_img.save(output_file)?;

    final_img
        .save(final_output_file)
        .context("error saving image")
}
