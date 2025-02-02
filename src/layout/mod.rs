use std::{sync::Arc, time::Instant};

use anyhow::Context;
use model::{ORTLayoutParser, ParseLayoutRequest};
use tokio::sync::mpsc::{self, Receiver, Sender};

pub mod model;

const MAX_CONCURRENT_LAYOUT_REQS: usize = 64;

#[derive(Debug, Clone)]
pub struct ParseLayoutQueue {
    queue: Sender<ParseLayoutRequest>,
}

impl ParseLayoutQueue {
    pub fn new(layout_parser: Arc<ORTLayoutParser>) -> Self {
        let (queue_sender, queue_receiver) = mpsc::channel(MAX_CONCURRENT_LAYOUT_REQS);

        tokio::spawn(start_layout_parser(layout_parser, queue_receiver));
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
    // TODO:  Batch of requests can be sent
    while let Some(ParseLayoutRequest {
        page_id,
        page_image,
        downscale_factor,
        metadata,
    }) = input_rx.recv().await
    {
        let queue_time = metadata.queue_time.elapsed().as_micros();
        let parser = Arc::clone(&layout_parser);
        // TODO:  create session options to cancel inference if sender withdraws
        // TODO: Add metadata to inference response
        tokio::spawn(async move {
            let start = Instant::now();
            let layout_result = parser
                .parse_layout_async(&page_image, downscale_factor)
                .await;
            let inference_duration = start.elapsed().as_millis();
            metadata
                .response_tx
                .send(layout_result)
                .expect("can't send layout result");
        });
    }
}
