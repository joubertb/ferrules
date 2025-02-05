use std::{ops::Range, sync::Arc, time::Duration};

use anyhow::Context;
use image::DynamicImage;
use pdfium_render::prelude::{PdfPageTextChar, Pdfium};

use crate::{
    entities::{BBox, CharSpan, Line, PageID},
    layout::model::ORTLayoutParser,
};
use tokio::sync::mpsc::{self, Receiver, Sender};

use super::page::parse_page_native;

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
            // TODO: should be passed in
            required_raster_width: ORTLayoutParser::REQUIRED_WIDTH,
            required_raster_height: ORTLayoutParser::REQUIRED_HEIGHT,
            sender_tx,
        }
    }
}

pub struct ParseNativeMetadata {
    time: Duration,
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
}

#[derive(Debug, Clone)]
pub struct ParseNativeQueue {
    queue: Sender<ParseNativeRequest>,
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
        self.queue
            .send(req)
            .await
            .context("error sending parse native request")
    }
}

fn handle_parse_native_req(pdfium: &Pdfium, req: ParseNativeRequest) -> anyhow::Result<()> {
    let ParseNativeRequest {
        doc_data,
        password,
        flatten,
        page_range,
        required_raster_width,
        required_raster_height,
        sender_tx,
    } = req;
    let mut document = pdfium.load_pdf_from_byte_slice(&doc_data, password.as_deref())?;

    let mut pages: Vec<_> = document.pages_mut().iter().enumerate().collect();
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

pub fn start_native_parser(mut input_rx: Receiver<ParseNativeRequest>) {
    let pdfium = Pdfium::new(
        Pdfium::bind_to_statically_linked_library().expect("can't load pdfiurm bindings"),
    );
    while let Some(req) = input_rx.blocking_recv() {
        match handle_parse_native_req(&pdfium, req) {
            Ok(_) => {}
            Err(e) => eprintln!("error parsing request natively : {:?}", e),
        }
    }
}
