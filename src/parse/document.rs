use std::{fmt::Write, ops::Range, path::Path, sync::Arc, time::Instant};

use futures::future::join_all;
use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use itertools::Itertools;
use pdfium_render::prelude::{PdfPage, Pdfium};
use uuid::Uuid;

use crate::{
    entities::{Document, Page, PageID, StructuredPage},
    layout::{model::ORTLayoutParser, ParseLayoutQueue},
    sanitize_doc_name,
};

use super::{
    merge::merge_elements_into_blocks,
    page::{parse_page_async, parse_pages},
};

use futures::stream::FuturesUnordered;
use futures::StreamExt;

pub async fn parse_document_pages_unordered<'a>(
    pages: &mut [(PageID, PdfPage<'a>)],
    layout_queue: ParseLayoutQueue,
    tmp_dir: &Path,
    flatten_pdf: bool,
    debug: bool,
    pb: ProgressBar,
) -> Vec<StructuredPage> {
    let mut tasks = FuturesUnordered::new();
    for (page_id, pdf_page) in pages {
        tasks.push(parse_page_async(
            *page_id,
            pdf_page,
            tmp_dir,
            flatten_pdf,
            layout_queue.clone(),
            debug,
            |_s| {
                pb.set_message(format!("Page #{}", *page_id + 1));
                pb.inc(1u64);
            },
        ));
    }

    let mut parsed_pages = Vec::new();
    while let Some(result) = tasks.next().await {
        // This page’s parse just finished, handle it now.
        match result {
            Ok(page) => {
                parsed_pages.push(page);
            }
            Err(e) => {
                tracing::error!("Error parsing page : {e:?}")
            }
        }
    }
    parsed_pages
}

pub async fn parse_document_pages<'a>(
    pages: &mut [(PageID, PdfPage<'a>)],
    layout_queue: ParseLayoutQueue,
    tmp_dir: &Path,
    flatten_pdf: bool,
    debug: bool,
    pb: ProgressBar,
) -> Vec<StructuredPage> {
    let parsed_pages_fut = pages
        .iter_mut()
        .map(|(page_idx, pdf_page)| {
            parse_page_async(
                *page_idx,
                pdf_page,
                tmp_dir,
                flatten_pdf,
                layout_queue.clone(),
                debug,
                |_s| {
                    pb.set_message(format!("Page #{}", *page_idx + 1));
                    pb.inc(1u64);
                },
            )
        })
        .collect::<Vec<_>>();

    let parsed_pages: Result<Vec<_>, _> = join_all(parsed_pages_fut).await.into_iter().collect();

    parsed_pages
        .map(|ppages| {
            ppages
                .into_iter()
                .sorted_by(|p1, p2| p1.id.cmp(&p2.id))
                .collect::<Vec<_>>()
        })
        .expect("error occured while parsing pages")
}

pub async fn parse_document_async<P: AsRef<Path>>(
    path: P,
    password: Option<&str>,
    flatten_pdf: bool,
    page_range: Option<Range<usize>>,
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
    let pdfium = Pdfium::new(Pdfium::bind_to_statically_linked_library()?);

    let mut document = pdfium.load_pdf_from_file(&path, password)?;
    let mut pages: Vec<_> = document.pages_mut().iter().enumerate().collect();
    let mut pages = if let Some(range) = page_range {
        if range.end > pages.len() {
            anyhow::bail!(
                "Page range end ({}) exceeds document length ({})",
                range.end,
                pages.len()
            );
        }
        pages.drain(range).collect()
    } else {
        pages
    };

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

    // Start layout model in separate task
    let layout_model = Arc::new(ORTLayoutParser::new().expect("can't load layout model"));
    let layout_queue = ParseLayoutQueue::new(layout_model);

    let parsed_pages = parse_document_pages_unordered(
        &mut pages,
        layout_queue,
        &tmp_dir,
        flatten_pdf,
        debug,
        pb.clone(),
    )
    .await;

    let all_elements = parsed_pages
        .iter()
        // TODO: clone might be huge here
        .flat_map(|p| p.elements.clone())
        .collect::<Vec<_>>();

    let pages = parsed_pages
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
        pages,
        blocks,
        debug_path: if debug { Some(tmp_dir) } else { None },
    })
}

pub fn parse_document<P: AsRef<Path>>(
    path: P,
    layout_model: &ORTLayoutParser,
    password: Option<&str>,
    flatten_pdf: bool,
    page_range: Option<Range<usize>>,
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
    let pdfium = Pdfium::new(Pdfium::bind_to_statically_linked_library()?);

    let mut document = pdfium.load_pdf_from_file(&path, password)?;
    let mut pages: Vec<_> = document.pages_mut().iter().enumerate().collect();
    let mut pages = if let Some(range) = page_range {
        if range.end > pages.len() {
            anyhow::bail!(
                "Page range end ({}) exceeds document length ({})",
                range.end,
                pages.len()
            );
        }
        pages.drain(range).collect()
    } else {
        pages
    };

    let chunk_size = std::thread::available_parallelism()
        .map(|c| c.get())
        .unwrap_or(4usize);

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

    let parsed_pages = pages
        .chunks_mut(chunk_size)
        .flat_map(|chunk| parse_pages(chunk, layout_model, &tmp_dir, flatten_pdf, debug, &pb))
        .flatten()
        .collect::<Vec<_>>();

    // TODO: clone might be huge here
    let all_elements = parsed_pages
        .iter()
        .flat_map(|p| p.elements.clone())
        .collect::<Vec<_>>();

    let pages = parsed_pages
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
        pages,
        blocks,
        debug_path: if debug { Some(tmp_dir) } else { None },
    })
}
