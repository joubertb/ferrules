use std::{sync::Arc, time::Instant};

use anyhow::Context;
use model::{ORTLayoutParser, ParseLayoutRequest};
use tokio::sync::mpsc::{self, Receiver, Sender};

pub mod model;

const MAX_CONCURRENT_LAYOUT_REQS: usize = ORTLayoutParser::ORT_INTRATHREAD;

#[derive(Debug, Clone)]
pub struct ParseLayoutQueue {
    queue: Sender<ParseLayoutRequest>,
}

impl ParseLayoutQueue {
    pub fn new(layout_parser: Arc<ORTLayoutParser>) -> Self {
        let (queue_sender, queue_receiver) = mpsc::channel(MAX_CONCURRENT_LAYOUT_REQS);

        tokio::task::spawn(start_layout_parser(layout_parser, queue_receiver));
        Self {
            queue: queue_sender,
        }
    }

    pub(crate) async fn push(&self, req: ParseLayoutRequest) -> anyhow::Result<()> {
        self.queue
            .send(req)
            .await
            .context("error sending  parse req")
    }
}

async fn start_layout_parser(
    layout_parser: Arc<ORTLayoutParser>,
    mut input_rx: Receiver<ParseLayoutRequest>,
) {
    while let Some(req) = input_rx.recv().await {
        let queue_time = req.metadata.queue_time.elapsed().as_micros();
        let page_id = req.page_id;
        tracing::info!("layout request queue time for page {page_id} took: {queue_time} us");
        tokio::spawn(handle_request(layout_parser.clone(), req));
    }
}

async fn handle_request(parser: Arc<ORTLayoutParser>, req: ParseLayoutRequest) {
    let ParseLayoutRequest {
        page_id,
        page_image,
        downscale_factor,
        metadata,
    } = req;

    let start = Instant::now();
    let layout_result = parser
        .parse_layout_async(&page_image, downscale_factor)
        .await;
    let inference_duration = start.elapsed().as_millis();
    tracing::info!("layout inference time for page {page_id} took: {inference_duration} ms");
    // Once you have the result:
    metadata
        .response_tx
        .send(layout_result)
        .expect("can't send parsed result over oneshot chan");
}
