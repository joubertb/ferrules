use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use anyhow::Context;
use image::DynamicImage;
use pdfium_render::prelude::{PdfPage, PdfRenderConfig};

use crate::{
    draw::{draw_blocks, draw_layout_bboxes, draw_text_lines},
    entities::{BBox, Element, Line, PageID, StructuredPage},
    layout::{
        model::{LayoutBBox, Metadata, ParseLayoutRequest},
        ParseLayoutQueue,
    },
    ocr::parse_image_ocr,
};

use super::{
    merge::{merge_elements_into_blocks, merge_lines_layout, merge_remaining},
    native::{parse_text_lines, parse_text_spans, ParseNativeMetadata, ParseNativePageResult},
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
    page_layout: &[LayoutBBox],
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
    native_text_lines: Vec<Line>,
    page_layout: &[LayoutBBox],
    page_image: &DynamicImage,
    downscale_factor: f32,
) -> anyhow::Result<(Vec<Line>, bool)> {
    let text_layout_box: Vec<&LayoutBBox> =
        page_layout.iter().filter(|b| b.is_text_block()).collect();
    let need_ocr = page_needs_ocr(&text_layout_box, &native_text_lines);

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
        native_text_lines
    };
    Ok((lines, need_ocr))
}

pub(crate) fn parse_page_native(
    page_id: PageID,
    page: &mut PdfPage,
    flatten_page: bool,
    required_raster_width: u32,
    required_raster_height: u32,
) -> anyhow::Result<ParseNativePageResult> {
    let start_time = Instant::now();
    if flatten_page {
        page.flatten()?;
    }
    let rescale_factor = {
        let scale_w = required_raster_width as f32 / page.width().value;
        let scale_h = required_raster_height as f32 / page.height().value;
        f32::min(scale_h, scale_w)
    };
    let downscale_factor = 1f32 / rescale_factor;

    let page_bbox = BBox {
        x0: 0f32,
        y0: 0f32,
        x1: page.width().value,
        y1: page.height().value,
    };
    let page_image = page
        .render_with_config(&PdfRenderConfig::default().scale_page_by_factor(rescale_factor))
        .map(|bitmap| bitmap.as_image())?;

    let page_image_scale1 = page
        .render_with_config(&PdfRenderConfig::default().scale_page_by_factor(1f32))
        .map(|bitmap| bitmap.as_image())?;

    let text_spans = parse_text_spans(page.text()?.chars().iter(), &page_bbox);
    let text_lines = parse_text_lines(text_spans);
    let time = start_time.elapsed();
    tracing::debug!(
        "Parsing page {} using pdfium took {}ms",
        page_id,
        time.as_millis()
    );
    Ok(ParseNativePageResult {
        page_id,
        text_lines,
        page_bbox,
        page_image: Arc::new(page_image),
        page_image_scale1,
        downscale_factor,
        _metadata: ParseNativeMetadata { time },
    })
}

pub async fn parse_page(
    parse_native_result: ParseNativePageResult,
    tmp_dir: PathBuf,
    layout_queue: ParseLayoutQueue,
    debug: bool,
) -> anyhow::Result<StructuredPage> {
    let ParseNativePageResult {
        page_id,
        text_lines,
        page_bbox,
        page_image,
        page_image_scale1,
        downscale_factor,
        _metadata: _,
    } = parse_native_result;
    let (layout_tx, layout_rx) = tokio::sync::oneshot::channel();

    let layout_req = ParseLayoutRequest {
        page_id,
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
        .context("error receiving layout on oneshot channel")?
        .context("error parsing page")?;
    tracing::debug!("Received layout parsing result for {page_id}");

    let (text_lines, need_ocr) =
        parse_page_text(text_lines, &page_layout, &page_image, downscale_factor)?;

    // Merging elements with layout
    let elements = build_page_elements(&page_layout, &text_lines, page_id)?;

    if debug {
        debug_page(
            &tmp_dir,
            page_id,
            &page_image_scale1,
            &text_lines,
            need_ocr,
            &page_layout,
            &elements,
        )?
    };

    let structured_page = StructuredPage {
        id: page_id,
        width: page_bbox.width(),
        height: page_bbox.height(),
        image: page_image_scale1,
        elements,
        need_ocr,
    };

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
