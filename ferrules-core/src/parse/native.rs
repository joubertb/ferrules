use std::{ops::Range, sync::Arc, time::Instant};

use anyhow::Context;
use image::DynamicImage;
use pdfium_render::prelude::{PdfPage, PdfPageTextChar, PdfRenderConfig, Pdfium};
use tracing::{instrument, Span};

use crate::{
    entities::{BBox, CharSpan, Line, PageID},
    layout::model::ORTLayoutParser,
};
use tokio::sync::mpsc::{self, Receiver, Sender};

const MAX_CONCURRENT_NATIVE_REQS: usize = 10;

pub(crate) fn parse_text_spans<'a>(
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

pub(crate) fn parse_text_lines(spans: Vec<CharSpan>) -> Vec<Line> {
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

pub struct ParseNativeRequest {
    pub doc_data: std::sync::Arc<[u8]>,
    pub password: Option<String>,
    pub flatten: bool,
    pub page_range: Option<Range<usize>>,
    pub required_raster_width: u32,
    pub required_raster_height: u32,
    pub sender_tx: Sender<anyhow::Result<ParseNativePageResult>>,
    pub count_only: bool,
}
impl ParseNativeRequest {
    pub fn new(
        data: &[u8],
        password: Option<&str>,
        flatten: bool,
        page_range: Option<Range<usize>>,
        sender_tx: Sender<anyhow::Result<ParseNativePageResult>>,
    ) -> Self {
        ParseNativeRequest {
            doc_data: Arc::from(data),
            password: password.map(|p| p.to_string()),
            flatten,
            page_range,
            // TODO: should be global?
            required_raster_width: ORTLayoutParser::REQUIRED_WIDTH,
            required_raster_height: ORTLayoutParser::REQUIRED_HEIGHT,
            sender_tx,
            count_only: false,
        }
    }

    pub fn new_count_only(
        data: &[u8],
        password: Option<&str>,
        sender_tx: Sender<anyhow::Result<ParseNativePageResult>>,
    ) -> Self {
        ParseNativeRequest {
            doc_data: Arc::from(data),
            password: password.map(|p| p.to_string()),
            flatten: false,            // Not needed for counting
            page_range: None,          // Count all pages
            required_raster_width: 0,  // Not needed for counting
            required_raster_height: 0, // Not needed for counting
            sender_tx,
            count_only: true,
        }
    }
}

#[derive(Debug)]
pub struct ParseNativeMetadata {
    pub parse_native_duration_ms: u128,
}

#[derive(Debug)]
pub struct ParseNativePageResult {
    // TODO: page_native_rotation
    pub page_id: PageID,
    pub text_lines: Vec<Line>,
    pub page_bbox: BBox,
    pub page_image: Arc<DynamicImage>,
    pub page_image_scale1: DynamicImage,
    pub downscale_factor: f32,
    pub metadata: ParseNativeMetadata,
    pub is_count_result: bool,
    pub total_page_count: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct ParseNativeQueue {
    queue: Sender<(ParseNativeRequest, Span)>,
}

impl Default for ParseNativeQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl ParseNativeQueue {
    pub fn new() -> Self {
        let (queue_sender, queue_receiver) = mpsc::channel(MAX_CONCURRENT_NATIVE_REQS);

        tokio::task::spawn_blocking(move || start_native_parser(queue_receiver));
        Self {
            queue: queue_sender,
        }
    }

    pub(crate) async fn push(&self, req: ParseNativeRequest) -> anyhow::Result<()> {
        let span = Span::current();
        self.queue
            .send((req, span))
            .await
            .context("error sending parse native request")
    }
}

#[instrument(skip(page))]
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

    let parse_native_duration_ms = start_time.elapsed().as_millis();
    tracing::debug!(
        "Parsing page {} using pdfium took {}ms",
        page_id,
        parse_native_duration_ms
    );
    Ok(ParseNativePageResult {
        page_id,
        text_lines,
        page_bbox,
        page_image: Arc::new(page_image),
        page_image_scale1,
        downscale_factor,
        metadata: ParseNativeMetadata {
            parse_native_duration_ms,
        },
        is_count_result: false,
        total_page_count: None,
    })
}

fn handle_parse_native_req(
    pdfium: &Pdfium,
    req: ParseNativeRequest,
    parent_span: Span,
) -> anyhow::Result<()> {
    // Reinter span
    let _guard = parent_span.enter();
    let ParseNativeRequest {
        doc_data,
        password,
        flatten,
        page_range,
        required_raster_width,
        required_raster_height,
        sender_tx,
        count_only,
    } = req;
    let mut document = pdfium.load_pdf_from_byte_slice(&doc_data, password.as_deref())?;
    let mut pages: Vec<_> = document.pages_mut().iter().enumerate().collect();

    // If only counting pages, send the count and return early
    if count_only {
        let total_pages = pages.len();
        use image::{DynamicImage, ImageBuffer};
        let dummy_image = DynamicImage::ImageRgb8(ImageBuffer::new(1, 1));

        let count_result = ParseNativePageResult {
            page_id: 0,
            text_lines: Vec::new(),
            page_bbox: crate::entities::BBox {
                x0: 0.0,
                y0: 0.0,
                x1: 0.0,
                y1: 0.0,
            },
            page_image: Arc::new(dummy_image.clone()),
            page_image_scale1: dummy_image,
            downscale_factor: 1.0,
            metadata: ParseNativeMetadata {
                parse_native_duration_ms: 0,
            },
            is_count_result: true,
            total_page_count: Some(total_pages),
        };
        sender_tx.blocking_send(Ok(count_result))?;
        return Ok(());
    }

    let pages = if let Some(range) = page_range {
        if range.end > pages.len() {
            anyhow::bail!(
                "Page range end ({}) exceeds document length ({})",
                range.end,
                pages.len()
            )
        }
        pages.drain(range).collect()
    } else {
        pages
    };
    for (page_id, mut page) in pages {
        let parsing_result = parse_page_native(
            page_id,
            &mut page,
            flatten,
            required_raster_width,
            required_raster_height,
        );
        sender_tx.blocking_send(parsing_result)?
    }
    Ok(())
}

pub fn start_native_parser(mut input_rx: Receiver<(ParseNativeRequest, Span)>) {
    let pdfium = Pdfium::new(
        Pdfium::bind_to_statically_linked_library().expect("can't load pdfiurm bindings"),
    );
    while let Some((req, parent_span)) = input_rx.blocking_recv() {
        match handle_parse_native_req(&pdfium, req, parent_span) {
            Ok(_) => {}
            Err(e) => eprintln!("error parsing request natively : {:?}", e),
        }
    }
}
