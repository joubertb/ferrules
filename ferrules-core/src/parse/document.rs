use std::{path::PathBuf, sync::Arc, time::Instant};

use std::ops::Range;

use anyhow::Context;
use tokio::{sync::mpsc, task::JoinSet};
use tracing::Instrument;

use super::native::{ParseNativeQueue, ParseNativeRequest};
use super::{
    merge::merge_elements_into_blocks, native::ParseNativePageResult, page::parse_page_full,
    titles::title_levels_kmeans,
};
use crate::entities::DocumentMetadata;
use crate::{
    entities::{ElementType, Page, PageID, ParsedDocument, StructuredPage},
    layout::{
        model::{ORTConfig, ORTLayoutParser},
        ParseLayoutQueue,
    },
};

/// Configuration options for parsing documents with FerrulesParser
#[derive(Debug, Clone)]
pub struct FerrulesParseConfig<'a> {
    /// Optional password for encrypted PDF documents
    pub password: Option<&'a str>,

    /// Whether to flatten the PDF before parsing. When true, flattens form fields and annotations
    /// into the document content for more consistent parsing results
    pub flatten_pdf: bool,

    /// Optional range of pages to parse. When None, parses all pages
    /// The range uses 0-based indexing (e.g., 0..5 parses first 5 pages)
    pub page_range: Option<std::ops::Range<usize>>,

    /// Optional directory path for debug output. When provided, saves intermediate parsing
    /// results and visualizations to this directory
    pub debug_dir: Option<std::path::PathBuf>,
}

impl Default for FerrulesParseConfig<'_> {
    fn default() -> Self {
        Self {
            password: None,
            flatten_pdf: true,
            page_range: None,
            debug_dir: None,
        }
    }
}

async fn parse_task<F, C>(
    parse_native_result: ParseNativePageResult,
    layout_queue: ParseLayoutQueue,
    debug_dir: Option<PathBuf>,
    callback: Option<F>,
    cancellation_callback: Option<C>,
) -> anyhow::Result<StructuredPage>
where
    F: FnOnce(PageID) + Send + 'static + Clone,
    C: Fn() -> bool + Send + Sync + 'static + Clone,
{
    let page_id = parse_native_result.page_id;

    // Check for cancellation before processing this page
    if let Some(ref cancel_cb) = cancellation_callback {
        if cancel_cb() {
            return Err(anyhow::anyhow!("Page processing was cancelled"));
        }
    }

    let result = parse_page_full(
        parse_native_result,
        debug_dir,
        layout_queue.clone(),
        cancellation_callback.clone(),
    )
    .await;

    // Check for cancellation after processing
    if let Some(ref cancel_cb) = cancellation_callback {
        if cancel_cb() {
            return Err(anyhow::anyhow!("Page processing was cancelled"));
        }
    }

    if let Some(callback) = callback {
        callback(page_id)
    }
    result
}

/// Core class Document parser that extracts structured content from PDF documents.
///
/// FerrulesParser uses a combination of native PDF parsing and machine learning-based
/// layout analysis to extract text, structural elements, and content hierarchies from documents.
#[derive(Clone)]
pub struct FerrulesParser {
    layout_queue: ParseLayoutQueue,
    native_queue: ParseNativeQueue,
}

impl FerrulesParser {
    /// Creates a new FerrulesParser instance with the specified layout model configuration
    ///
    /// # Arguments
    /// * `layout_config` - Configuration for the ONNX Runtime layout analysis model
    ///
    /// # Returns
    /// A new FerrulesParser instance
    ///
    /// # Panics
    /// Panics if the layout model cannot be loaded with the given configuration
    pub fn new(layout_config: ORTConfig) -> Self {
        let layout_model =
            Arc::new(ORTLayoutParser::new(layout_config).expect("can't load layout model"));
        let native_queue = ParseNativeQueue::new();
        let layout_queue = ParseLayoutQueue::new(layout_model);
        Self {
            layout_queue,
            native_queue,
        }
    }

    /// Gets the total number of pages in a PDF document without full processing
    ///
    /// # Arguments
    /// * `doc` - Raw bytes of the PDF document
    /// * `password` - Optional password for encrypted PDFs
    ///
    /// # Returns
    /// A Result containing the total page count or an error
    ///
    /// # Example
    /// ```no_run
    /// use ferrules_core::{FerrulesParser, layout::model::ORTConfig};
    ///
    /// async fn get_count() {
    ///     let parser = FerrulesParser::new(ORTConfig::default());
    ///     let doc_bytes = std::fs::read("document.pdf").unwrap();
    ///     let page_count = parser.get_page_count(&doc_bytes, None).await.unwrap();
    ///     println!("Document has {} pages", page_count);
    /// }
    /// ```
    pub async fn get_page_count(
        &self,
        doc: &[u8],
        password: Option<&str>,
    ) -> anyhow::Result<usize> {
        use super::native::ParseNativeRequest;
        use tokio::sync::mpsc;

        // Create a channel to receive the count result
        let (result_tx, mut result_rx) = mpsc::channel(1);

        // Create a count-only request
        let request = ParseNativeRequest::new_count_only(doc, password, result_tx);

        // Send the request to the native queue
        self.native_queue
            .push(request)
            .await
            .context("Failed to send page count request to native queue")?;

        // Wait for the result
        let result = result_rx
            .recv()
            .await
            .context("Failed to receive page count result")?
            .context("Native parsing error")?;

        // Extract the page count from the result
        if result.is_count_result {
            result
                .total_page_count
                .context("Count result missing total_page_count")
        } else {
            anyhow::bail!("Received non-count result for page count request")
        }
    }

    /// Parses a document into a structured format with optional page-level progress callback
    ///
    /// # Arguments
    /// * `doc` - Raw bytes of the document to parse
    /// * `doc_name` - Name of the document
    /// * `config` - Parsing configuration options
    /// * `page_callback` - Optional callback function called after each page is processed
    ///
    /// # Returns
    /// A Result containing the parsed document structure or an error
    ///
    /// # Examples
    /// ```no_run
    /// use ferrules_core::{FerrulesParser, FerrulesParseConfig, layout::model::ORTConfig};
    ///
    /// async fn parse() {
    ///     let parser = FerrulesParser::new(ORTConfig::default());
    ///     let config = FerrulesParseConfig::default();
    ///
    ///     let doc_bytes = std::fs::read("document.pdf").unwrap();
    ///     let parsed = parser.parse_document(
    ///         &doc_bytes,
    ///         "document.pdf".to_string(),
    ///         config,
    ///         Some(|page_id| println!("Parsed page {}", page_id))
    ///     ).await.unwrap();
    /// }
    #[allow(clippy::too_many_arguments)]
    pub async fn parse_document<F, C>(
        &self,
        doc: &[u8],
        doc_name: String,
        config: FerrulesParseConfig<'_>,
        page_callback: Option<F>,
        cancellation_callback: Option<C>,
    ) -> anyhow::Result<ParsedDocument>
    where
        F: FnOnce(PageID) + Send + 'static + Clone,
        C: Fn() -> bool + Send + Sync + 'static + Clone,
    {
        let FerrulesParseConfig {
            password,
            flatten_pdf,
            page_range,
            debug_dir,
        } = config;
        let start_time = Instant::now();
        let parsed_pages = self
            .parse_doc_pages(
                doc,
                flatten_pdf,
                password,
                page_range,
                debug_dir.clone(),
                page_callback,
                cancellation_callback.clone(),
            )
            .await?;

        let all_elements = parsed_pages
            .iter()
            .flat_map(|p| p.elements.clone())
            .collect::<Vec<_>>();

        let titles = all_elements
            .iter()
            .filter(|e| matches!(e.kind, ElementType::Title | ElementType::Subtitle))
            .collect::<Vec<_>>();

        let title_level = title_levels_kmeans(&titles, 6);

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

        let blocks = merge_elements_into_blocks(all_elements, title_level)?;

        let duration = start_time.elapsed();

        Ok(ParsedDocument {
            doc_name,
            pages: doc_pages,
            blocks,
            debug_path: debug_dir,
            metadata: DocumentMetadata::new(duration),
        })
    }

    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(skip_all)]
    async fn parse_doc_pages<F, C>(
        &self,
        data: &[u8],
        flatten_pdf: bool,
        password: Option<&str>,
        page_range: Option<Range<usize>>,
        debug_dir: Option<PathBuf>,
        callback: Option<F>,
        cancellation_callback: Option<C>,
    ) -> anyhow::Result<Vec<StructuredPage>>
    where
        F: FnOnce(PageID) + Send + 'static + Clone,
        C: Fn() -> bool + Send + Sync + 'static + Clone,
    {
        // Check for cancellation before starting
        if let Some(ref cancel_cb) = cancellation_callback {
            if cancel_cb() {
                // Flush layout queue to stop background processing
                let _ = self.layout_queue.flush().await;
                return Err(anyhow::anyhow!("Document processing was cancelled"));
            }
        }

        let mut set = JoinSet::new();
        let (native_tx, mut native_rx) = mpsc::channel(32);
        let req = ParseNativeRequest::new(data, password, flatten_pdf, page_range, native_tx);
        self.native_queue.push(req).await?;

        while let Some(native_page) = native_rx.recv().await {
            // Check for cancellation before processing each page
            if let Some(ref cancel_cb) = cancellation_callback {
                if cancel_cb() {
                    // Flush layout queue to stop background processing
                    let _ = self.layout_queue.flush().await;
                    return Err(anyhow::anyhow!("Document processing was cancelled"));
                }
            }

            match native_page {
                Ok(parse_native_result) => {
                    let tmp_dir = debug_dir.clone();
                    let callback = callback.clone();
                    let cancel_cb_clone = cancellation_callback.clone();
                    set.spawn(
                        parse_task(
                            parse_native_result,
                            self.layout_queue.clone(),
                            tmp_dir,
                            callback,
                            cancel_cb_clone,
                        )
                        .in_current_span(),
                    );
                }
                Err(_) => eprintln!("Error occured parsing page in doc"),
            }
        }

        // Get results
        let mut parsed_pages = Vec::new();
        while let Some(result) = set.join_next().await {
            // Check for cancellation while collecting results
            if let Some(ref cancel_cb) = cancellation_callback {
                if cancel_cb() {
                    // Flush layout queue to stop background processing
                    let _ = self.layout_queue.flush().await;
                    return Err(anyhow::anyhow!("Document processing was cancelled"));
                }
            }

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
        parsed_pages.sort_by(|p1, p2| p1.id.cmp(&p2.id));
        Ok(parsed_pages)
    }
}
