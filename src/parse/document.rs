use std::{fmt::Write, ops::Range, path::Path, sync::Arc, time::Instant};

use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use memmap2::Mmap;
use pdfium_render::prelude::Pdfium;
use tokio::{fs::File, sync::mpsc, task::JoinSet};
use uuid::Uuid;

use crate::{
    entities::{Document, Page, StructuredPage},
    layout::{model::ORTLayoutParser, ParseLayoutQueue},
    sanitize_doc_name,
};

use super::{
    merge::merge_elements_into_blocks,
    native::{ParseNativeQueue, ParseNativeRequest},
    page::parse_page,
};

#[allow(clippy::too_many_arguments)]
async fn parse_document_pages_unordered(
    data: &[u8],
    flatten_pdf: bool,
    password: Option<&str>,
    page_range: Option<Range<usize>>,
    tmp_dir: &Path,
    debug: bool,
    layout_queue: ParseLayoutQueue,
    native_queue: ParseNativeQueue,
    pb: ProgressBar,
) -> anyhow::Result<Vec<StructuredPage>> {
    let mut set = JoinSet::new();
    let (native_tx, mut native_rx) = mpsc::channel(32);
    let req = ParseNativeRequest::new(data, password, flatten_pdf, page_range, native_tx);
    native_queue.push(req).await?;
    while let Some(native_page) = native_rx.recv().await {
        match native_page {
            Ok(parse_native_result) => {
                pb.set_message(format!("Page #{}", parse_native_result.page_id + 1));
                pb.inc(1u64);
                set.spawn(parse_page(
                    parse_native_result,
                    tmp_dir.to_path_buf(),
                    layout_queue.clone(),
                    debug,
                ));
            }
            Err(_) => todo!(),
        }
    }

    // Get results
    let mut parsed_pages = Vec::new();
    while let Some(result) = set.join_next().await {
        match result {
            Ok(Ok(page)) => {
                parsed_pages.push(page);
            }
            Ok(Err(e)) => {
                tracing::error!("Error parsing page : {e:?}")
            }
            Err(e) => {
                tracing::error!("Error Joining : {e:?}")
            }
        }
    }
    Ok(parsed_pages)
}

fn get_doc_length(
    doc_data: &[u8],
    password: Option<&str>,
    page_range: Option<Range<usize>>,
) -> usize {
    // TODO : This panic ! should be handlered
    let pdfium = Pdfium::new(Pdfium::bind_to_statically_linked_library().unwrap());
    let document = pdfium.load_pdf_from_byte_slice(doc_data, password).unwrap();
    let pages: Vec<_> = document.pages().iter().enumerate().collect();
    match page_range {
        Some(range) => {
            if range.end > pages.len() {
                panic!(
                    "Page range end ({}) exceeds document length ({})",
                    range.end,
                    pages.len()
                );
            }
            range.len()
        }
        None => pages.len(),
    }
}

pub async fn parse_document_async<P: AsRef<Path>>(
    path: P,
    password: Option<&str>,
    flatten_pdf: bool,
    page_range: Option<Range<usize>>,
    layout_model: Arc<ORTLayoutParser>,
    debug: bool,
) -> anyhow::Result<Document<P>> {
    let start_time = Instant::now();
    let doc_name = path
        .as_ref()
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.split('.').next().map(|s| s.to_owned()))
        .unwrap_or(Uuid::new_v4().to_string());

    let tmp_dir = std::env::temp_dir().join(format!("ferrules-{}", sanitize_doc_name(&doc_name)));
    if debug {
        std::fs::create_dir_all(&tmp_dir)?;
    }
    // TODO : refac memap
    let file = File::open(&path).await?;
    let mmap = unsafe { Mmap::map(&file)? };
    let length_pages = get_doc_length(&mmap, password, page_range.clone());

    let pb = ProgressBar::new(length_pages as u64);

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

    // Start layout model in separate task
    let layout_queue = ParseLayoutQueue::new(layout_model);
    let native_queue = ParseNativeQueue::new();

    let parsed_pages = parse_document_pages_unordered(
        &mmap,
        flatten_pdf,
        password,
        page_range,
        &tmp_dir,
        debug,
        layout_queue,
        native_queue,
        pb.clone(),
    )
    .await?;

    let all_elements = parsed_pages
        .iter()
        // TODO: clone might be huge here
        .flat_map(|p| p.elements.clone())
        .collect::<Vec<_>>();

    let doc_pages = parsed_pages
        .into_iter()
        .map(|sp| Page {
            id: sp.id,
            width: sp.width,
            height: sp.height,
            need_ocr: sp.need_ocr,
            image: sp.image,
        })
        .collect();

    let blocks = merge_elements_into_blocks(all_elements)?;

    let duration = Instant::now().duration_since(start_time).as_millis();
    pb.finish_with_message(format!("Parsed document in {}ms", duration));

    Ok(Document {
        path,
        doc_name,
        pages: doc_pages,
        blocks,
        debug_path: if debug { Some(tmp_dir) } else { None },
    })
}
