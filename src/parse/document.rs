use std::{fmt::Write, ops::Range, path::Path, time::Instant};

use memmap2::Mmap;
use pdfium_render::prelude::Pdfium;
use tokio::{fs::File, sync::mpsc, task::JoinSet};
use uuid::Uuid;

use crate::{
    entities::{Document, DocumentMetadata, Page, PageID, StructuredPage},
    layout::ParseLayoutQueue,
    sanitize_doc_name,
};

use super::{
    merge::merge_elements_into_blocks,
    native::{ParseNativeQueue, ParseNativeRequest},
    page::parse_page,
};

#[allow(clippy::too_many_arguments)]
async fn parse_doc_pages<F>(
    data: &[u8],
    flatten_pdf: bool,
    password: Option<&str>,
    page_range: Option<Range<usize>>,
    tmp_dir: &Path,
    debug: bool,
    layout_queue: ParseLayoutQueue,
    native_queue: ParseNativeQueue,
    callback: Option<F>,
) -> anyhow::Result<Vec<StructuredPage>>
where
    // TODO: callback on function result
    F: FnOnce(PageID) + Send + 'static + Clone,
{
    let mut set = JoinSet::new();
    let (native_tx, mut native_rx) = mpsc::channel(32);
    let req = ParseNativeRequest::new(data, password, flatten_pdf, page_range, native_tx);
    native_queue.push(req).await?;
    while let Some(native_page) = native_rx.recv().await {
        match native_page {
            Ok(parse_native_result) => {
                let layout_queue = layout_queue.clone();
                let tmp_dir = tmp_dir.to_owned();
                let callback = callback.clone();
                set.spawn(async move {
                    let page_id = parse_native_result.page_id;
                    let result =
                        parse_page(parse_native_result, tmp_dir, layout_queue, debug).await;
                    if let Some(callback) = callback {
                        callback(page_id)
                    }
                    result
                });
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

pub fn get_doc_length<P: AsRef<Path>>(
    path: P,
    password: Option<&str>,
    page_range: Option<Range<usize>>,
) -> usize {
    // TODO : This panic ! should be handlered
    let pdfium = Pdfium::new(Pdfium::bind_to_statically_linked_library().unwrap());
    let document = pdfium.load_pdf_from_file(&path, password).unwrap();
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

#[allow(clippy::too_many_arguments)]
pub async fn parse_document<P: AsRef<Path>, F>(
    path: P,
    password: Option<&str>,
    flatten_pdf: bool,
    page_range: Option<Range<usize>>,
    layout_queue: ParseLayoutQueue,
    native_queue: ParseNativeQueue,
    debug: bool,
    page_callback: Option<F>,
) -> anyhow::Result<Document<P>>
where
    F: FnOnce(PageID) + Send + 'static + Clone,
{
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

    let parsed_pages = parse_doc_pages(
        &mmap,
        flatten_pdf,
        password,
        page_range,
        &tmp_dir,
        debug,
        layout_queue,
        native_queue,
        page_callback,
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

    let duration = start_time.elapsed();

    Ok(Document {
        path,
        doc_name,
        pages: doc_pages,
        blocks,
        debug_path: if debug { Some(tmp_dir) } else { None },
        metadata: DocumentMetadata::new(duration),
    })
}
