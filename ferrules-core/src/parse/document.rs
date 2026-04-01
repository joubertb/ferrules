use std::{collections::HashMap, ops::Range, path::PathBuf, sync::Arc, time::Instant};

use anyhow::Context;
use tokio::{sync::mpsc, task::JoinSet};
use tracing::Instrument;

use super::native::{ParseNativeQueue, ParseNativeRequest};
use super::{
    merge::merge_elements_into_blocks, native::ParseNativePageResult, page::parse_page_full,
    titles::title_levels_kmeans,
};
use crate::entities::DocumentMetadata;
use crate::error::FerrulesError;
use crate::{
    blocks::Block,
    debug::get_debug_context,
    debug_print,
    entities::{ElementType, Page, PageID, ParsedDocument, StructuredPage},
    layout::{
        model::{ORTConfig, ORTLayoutParser},
        ParseLayoutQueue,
    },
    metrics::ParsingMetrics,
    ocr::{OCRParser, OCRQueue},
    parse::table::{ParseTableQueue, TableParser, TableTransformer},
};

/// Diagnostic info about internal queue health
#[derive(Debug, Clone, serde::Serialize)]
pub struct QueueStatus {
    pub native_thread_alive: bool,
    pub native_queue_capacity: usize,
    pub native_queue_max_capacity: usize,
}

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

/// Result from PDF metadata extraction (page count and document metadata)
#[derive(Debug, Clone)]
pub struct PdfMetadataResult {
    /// Total number of pages in the document
    pub page_count: usize,
    /// PDF title from document metadata (if present)
    pub title: Option<String>,
}

async fn parse_task<F, C>(
    parse_native_result: ParseNativePageResult,
    layout_queue: ParseLayoutQueue,
    table_queue: ParseTableQueue,
    ocr_queue: OCRQueue,
    debug_dir: Option<PathBuf>,
    callback: Option<F>,
    cancellation_callback: Option<C>,
) -> Result<StructuredPage, FerrulesError>
where
    F: FnOnce(PageID) + Send + 'static + Clone,
    C: Fn() -> bool + Send + Sync + 'static + Clone,
{
    let page_id = parse_native_result.page_id;

    // Check for cancellation before processing this page
    if let Some(ref cancel_cb) = cancellation_callback {
        if cancel_cb() {
            return Err(FerrulesError::Cancelled);
        }
    }

    let result = parse_page_full(
        parse_native_result,
        debug_dir,
        layout_queue.clone(),
        table_queue.clone(),
        cancellation_callback.clone(),
        ocr_queue.clone(),
    )
    .await;

    // Check for cancellation after processing
    if let Some(ref cancel_cb) = cancellation_callback {
        if cancel_cb() {
            return Err(FerrulesError::Cancelled);
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
    table_queue: ParseTableQueue,
    ocr_queue: OCRQueue,
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
            Arc::new(ORTLayoutParser::new(layout_config.clone()).expect("can't load layout model"));
        let native_queue = ParseNativeQueue::new();
        let layout_queue = ParseLayoutQueue::new(layout_model);
        let transformer = TableTransformer::new(&layout_config).ok();
        let table_parser = Arc::new(TableParser::new(transformer));
        let table_queue = ParseTableQueue::new(table_parser);
        let ocr_parser = Arc::new(OCRParser::new());
        let ocr_queue = OCRQueue::new(ocr_parser);
        Self {
            layout_queue,
            native_queue,
            table_queue,
            ocr_queue,
        }
    }

    /// Returns diagnostic info about internal queue health
    pub fn queue_status(&self) -> QueueStatus {
        QueueStatus {
            native_thread_alive: self.native_queue.is_alive(),
            native_queue_capacity: self.native_queue.capacity(),
            native_queue_max_capacity: self.native_queue.max_capacity(),
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
        let request =
            ParseNativeRequest::new_count_only(doc, password, result_tx, get_debug_context());

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

    /// Gets PDF metadata including page count and title from the document.
    /// This is more efficient than calling get_page_count separately when you need both.
    pub async fn get_pdf_metadata(
        &self,
        doc: &[u8],
        password: Option<&str>,
    ) -> anyhow::Result<PdfMetadataResult> {
        use super::native::ParseNativeRequest;

        // Create a channel to receive the count result
        let (result_tx, mut result_rx) = mpsc::channel(1);

        // Create a count-only request
        let request =
            ParseNativeRequest::new_count_only(doc, password, result_tx, get_debug_context());

        // Send the request to the native queue
        self.native_queue
            .push(request)
            .await
            .context("Failed to send metadata request to native queue")?;

        // Wait for the result
        let result = result_rx
            .recv()
            .await
            .context("Failed to receive metadata result")?
            .context("Native parsing error")?;

        // Extract metadata from the result
        if result.is_count_result {
            let page_count = result
                .total_page_count
                .context("Count result missing total_page_count")?;
            Ok(PdfMetadataResult {
                page_count,
                title: result.pdf_title,
            })
        } else {
            anyhow::bail!("Received non-count result for metadata request")
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
    ///         Some(|page_id| println!("Parsed page {}", page_id)),
    ///         None::<fn() -> bool>,
    ///     ).await.unwrap();
    /// }
    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(skip(self, doc, page_callback, cancellation_callback), fields(doc_name = %doc_name))]
    pub async fn parse_document<F, C>(
        &self,
        doc: &[u8],
        doc_name: String,
        config: FerrulesParseConfig<'_>,
        page_callback: Option<F>,
        cancellation_callback: Option<C>,
    ) -> Result<ParsedDocument, FerrulesError>
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

        // Initialize universal font corrector with PDF data for direct glyph extraction
        #[cfg(feature = "correction-engine")]
        {
            use crate::font_analysis::initialize_universal_corrector;
            if let Err(e) = initialize_universal_corrector(doc) {
                tracing::warn!("Failed to initialize universal font corrector: {}", e);
            }
        }

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

        // Post-processing timing instrumentation
        let post_pages_start = Instant::now();

        // Element extraction
        let extract_start = Instant::now();
        let all_elements = parsed_pages
            .iter()
            .flat_map(|p| p.elements.clone())
            .collect::<Vec<_>>();
        tracing::info!(
            "⏱️ Element extraction took {:?} ({} elements)",
            extract_start.elapsed(),
            all_elements.len()
        );

        // Title analysis
        let title_start = Instant::now();
        let titles = all_elements
            .iter()
            .filter(|e| matches!(e.kind, ElementType::Title | ElementType::Subtitle))
            .collect::<Vec<_>>();

        let title_level = title_levels_kmeans(&titles, 6);
        tracing::info!(
            "⏱️ Title k-means took {:?} ({} titles)",
            title_start.elapsed(),
            titles.len()
        );

        // Page heights map
        let page_heights_start = Instant::now();
        let page_heights: HashMap<usize, f32> =
            parsed_pages.iter().map(|sp| (sp.id, sp.height)).collect();
        tracing::info!(
            "⏱️ Page heights map took {:?} ({} pages)",
            page_heights_start.elapsed(),
            page_heights.len()
        );

        // Convert to doc pages
        let doc_pages_start = Instant::now();
        let doc_pages = parsed_pages
            .iter()
            .map(|sp| Page {
                id: sp.id,
                width: sp.width,
                height: sp.height,
                need_ocr: sp.need_ocr,
                image: sp.image.clone(),
            })
            .collect();
        tracing::info!(
            "⏱️ Doc pages conversion took {:?}",
            doc_pages_start.elapsed()
        );

        // Merge elements into blocks (the slow one)
        let merge_start = Instant::now();
        let blocks = merge_elements_into_blocks(all_elements, title_level, page_heights)?;
        tracing::info!(
            "⏱️ merge_elements_into_blocks took {:?} ({} blocks)",
            merge_start.elapsed(),
            blocks.len()
        );

        tracing::info!(
            "⏱️ Total parse_document post-processing took {:?}",
            post_pages_start.elapsed()
        );

        if let Some(ref debug_dir) = debug_dir {
            self.save_debug_binary(debug_dir, &doc_name, &parsed_pages, &blocks);
        }

        let duration = start_time.elapsed();

        let parsing_metrics = ParsingMetrics {
            total_duration_ms: duration.as_secs_f64() * 1000.0,
            pages: parsed_pages.iter().map(|p| p.metrics.clone()).collect(),
        };

        Ok(ParsedDocument {
            doc_name,
            pages: doc_pages,
            blocks,
            debug_path: debug_dir,
            metadata: DocumentMetadata::new(duration),
            metrics: parsing_metrics,
        })
    }

    fn save_debug_binary(
        &self,
        debug_dir: &std::path::Path,
        doc_name: &str,
        parsed_pages: &[StructuredPage],
        blocks: &[Block],
    ) {
        let mut debug_pages = Vec::new();
        for sp in parsed_pages {
            let mut page_blocks = Vec::new();
            for block in blocks {
                if block.pages_id.contains(&sp.id) {
                    page_blocks.push(block.clone());
                }
            }

            let mut image_data = Vec::new();
            let _ = sp.image.write_to(
                &mut std::io::Cursor::new(&mut image_data),
                image::ImageFormat::Png,
            );

            debug_pages.push(crate::debug_info::DebugPage {
                page_number: sp.id,
                native_lines: sp.native_lines.clone(),
                paths: sp.paths.clone(),
                layout_bboxes: sp.layout.clone(),
                ocr_lines: sp.ocr_lines.clone(),
                elements: sp.elements.clone(),
                blocks: page_blocks,
                image_data,
                width: sp.width,
                height: sp.height,
            });
        }
        let debug_doc = crate::debug_info::DebugDocument {
            name: doc_name.to_string(),
            pages: debug_pages,
        };

        let debug_file = debug_dir.join(format!("{}.ferr", doc_name));
        let bytes = rkyv::to_bytes::<_, 1024>(&debug_doc).expect("failed to serialize debug doc");
        std::fs::write(debug_file, bytes).expect("failed to write debug file");
    }

    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(skip(self, data, callback, cancellation_callback), fields(flatten_pdf = flatten_pdf, page_range = ?page_range))]
    async fn parse_doc_pages<F, C>(
        &self,
        data: &[u8],
        flatten_pdf: bool,
        password: Option<&str>,
        page_range: Option<Range<usize>>,
        debug_dir: Option<PathBuf>,
        callback: Option<F>,
        cancellation_callback: Option<C>,
    ) -> Result<Vec<StructuredPage>, FerrulesError>
    where
        F: FnOnce(PageID) + Send + 'static + Clone,
        C: Fn() -> bool + Send + Sync + 'static + Clone,
    {
        // Check for cancellation before starting
        if let Some(ref cancel_cb) = cancellation_callback {
            if cancel_cb() {
                // Flush layout queue to stop background processing
                let _ = self.layout_queue.flush().await;
                return Err(FerrulesError::Cancelled);
            }
        }

        let mut set = JoinSet::new();
        let (native_tx, mut native_rx) = mpsc::channel(32);
        let req = ParseNativeRequest::new(
            data,
            password,
            flatten_pdf,
            page_range,
            native_tx,
            get_debug_context(),
        );
        self.native_queue.push(req).await?;

        while let Some(native_page) = native_rx.recv().await {
            // Check for cancellation before processing each page
            if let Some(ref cancel_cb) = cancellation_callback {
                if cancel_cb() {
                    // Flush layout queue to stop background processing
                    let _ = self.layout_queue.flush().await;
                    return Err(FerrulesError::Cancelled);
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
                            self.table_queue.clone(),
                            self.ocr_queue.clone(),
                            tmp_dir,
                            callback,
                            cancel_cb_clone,
                        )
                        .in_current_span(),
                    );
                }
                Err(_) => debug_print!("Error occured parsing page in doc"),
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
                    return Err(FerrulesError::Cancelled);
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
