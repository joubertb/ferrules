use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, State},
    http::{
        header::{ACCEPT, CONTENT_TYPE},
        HeaderMap, Response, StatusCode,
    },
    response::{IntoResponse, Sse},
    routing::{delete, get, post},
    Json, Router,
};
use axum_tracing_opentelemetry::middleware::OtelAxumLayer;
use clap::Parser;
use ferrules_api::init_tracing;
use ferrules_core::{
    debug::{
        cleanup_old_debug_files, clear_debug_context, delete_debug_file, init_debug_config,
        read_debug_file, set_debug_context, DebugOutput,
    },
    font_analysis::initialize_for_cli,
    layout::model::{ORTConfig, OrtExecutionProvider},
    render::markdown::to_markdown,
    utils::save_doc_images,
    FerrulesParseConfig, FerrulesParser,
};
use memmap2::Mmap;
use mimalloc::MiMalloc;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::{Seek, Write},
    path::PathBuf,
    sync::Arc,
};
use tempfile::NamedTempFile;
use tokio::{
    fs::File,
    net::TcpListener,
    sync::{mpsc, Mutex},
};
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

const MAX_SIZE_LIMIT: usize = 250 * 1024 * 1024;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// OpenTelemetry collector endpoint
    #[arg(long, env = "OTLP_ENDPOINT", default_value = "http://localhost:4317")]
    otlp_endpoint: String,

    /// Sentry DSN
    #[arg(long, env = "SENTRY_DSN")]
    sentry_dsn: Option<String>,

    /// Sentry environment
    #[arg(long, env = "SENTRY_ENVIRONMENT", default_value = "dev")]
    sentry_environment: String,

    /// API listen address
    #[arg(long, env = "API_LISTEN_ADDR", default_value = "0.0.0.0:3002")]
    listen_addr: String,

    /// Enable debug mode
    #[arg(long, env = "SENTRY_DEBUG", default_value = "false")]
    sentry_debug: bool,

    /// Debug output mode (none, stderr, file, both)
    #[arg(long, env = "FERRULES_DEBUG_OUTPUT", default_value = "none")]
    debug_output: String,

    /// Use CoreML for layout inference (default: true)
    #[arg(
            long,
            default_value_t = cfg!(target_os = "macos"),
            help = "Enable or disable the use of CoreML for layout inference"
        )]
    pub coreml: bool,

    #[arg(
        long,
        default_value_t = true,
        help = "Enable or disable Apple Neural Engine acceleration (only applies when CoreML is enabled)"
    )]
    pub use_ane: bool,

    #[arg(
        long,
        default_value_t = false,
        help = "Enable or disable the use of TensorRT for layout inference"
    )]
    pub trt: bool,

    #[arg(
        long,
        default_value_t = false,
        help = "Enable or disable the use of CUDA for layout inference"
    )]
    pub cuda: bool,

    /// CUDA device ID to use for GPU acceleration (e.g. 0 for first GPU)
    #[arg(
        long,
        help = "CUDA device ID to use (0 for first GPU)",
        default_value_t = 0
    )]
    pub device_id: i32,

    /// Number of threads to use within individual operations
    #[arg(
        long,
        short = 'j',
        help = "Number of threads to use for parallel processing within operations",
        default_value = "16"
    )]
    intra_threads: usize,

    /// Number of threads to use for parallel operation execution
    #[arg(
        long,
        help = "Number of threads to use for executing operations in parallel",
        default_value = "4"
    )]
    inter_threads: usize,

    #[arg(long, short = 'O', help = "Ort graph optimization level")]
    graph_opt_level: Option<usize>,
}

fn parse_ep_args(args: &Args) -> Vec<OrtExecutionProvider> {
    let mut providers = Vec::new();
    if args.trt {
        providers.push(OrtExecutionProvider::Trt(args.device_id));
    }
    if args.cuda {
        providers.push(OrtExecutionProvider::CUDA(args.device_id));
    }

    if args.coreml {
        providers.push(OrtExecutionProvider::CoreML {
            ane_only: args.use_ane,
        });
    }
    providers.push(OrtExecutionProvider::CPU);
    providers
}

#[derive(Debug, Serialize)]
struct ApiResponse<T> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ParseOptions {
    page_range: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum ParseEvent {
    #[serde(rename = "job_started")]
    JobStarted { job_id: Uuid },
    #[serde(rename = "progress")]
    Progress {
        pages_completed: usize,
        total_pages: usize,
        page_id: usize,
    },
    #[serde(rename = "complete")]
    Complete {
        document: serde_json::Value,
        total_pages: usize,
    },
    #[serde(rename = "cancelled")]
    Cancelled { message: String },
    #[serde(rename = "error")]
    Error { message: String },
}

#[derive(Debug)]
struct JobHandle {
    cancellation_token: CancellationToken,
    tx: mpsc::Sender<ParseEvent>,
}

#[derive(Debug, Clone)]
struct JobManager {
    active_jobs: Arc<Mutex<HashMap<Uuid, JobHandle>>>,
}

#[derive(Clone)]
struct AppState {
    parser: FerrulesParser,
    job_manager: JobManager,
}

impl JobManager {
    fn new() -> Self {
        Self {
            active_jobs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn start_job(&self, job_id: Uuid, tx: mpsc::Sender<ParseEvent>) -> CancellationToken {
        let cancellation_token = CancellationToken::new();
        let job_handle = JobHandle {
            cancellation_token: cancellation_token.clone(),
            tx,
        };

        let mut jobs = self.active_jobs.lock().await;
        jobs.insert(job_id, job_handle);
        tracing::info!("Started job {}", job_id);

        cancellation_token
    }

    async fn cancel_job(&self, job_id: Uuid) -> Result<(), String> {
        let jobs = self.active_jobs.lock().await;

        if let Some(job_handle) = jobs.get(&job_id) {
            job_handle.cancellation_token.cancel();

            // Send cancellation event
            let _ = job_handle
                .tx
                .send(ParseEvent::Cancelled {
                    message: "Job was cancelled by user request".to_string(),
                })
                .await;

            tracing::info!("Cancelled job {}", job_id);
            Ok(())
        } else {
            Err(format!("Job {job_id} not found or already completed"))
        }
    }

    async fn complete_job(&self, job_id: Uuid) {
        let mut jobs = self.active_jobs.lock().await;
        if jobs.remove(&job_id).is_some() {
            tracing::info!("Completed job {}", job_id);
        }
    }
}

/// Handler to retrieve debug output for a specific document
#[tracing::instrument(skip_all)]
async fn get_debug_handler(
    Path(doc_name): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiResponse<()>>)> {
    match read_debug_file(&doc_name) {
        Ok(content) => Ok(content),
        Err(e) => {
            if e.contains("not found") {
                Err((
                    StatusCode::NOT_FOUND,
                    Json(ApiResponse {
                        success: false,
                        data: None,
                        error: Some("Debug file not found".to_string()),
                    }),
                ))
            } else if e.contains("Path traversal")
                || e.contains("Path separators")
                || e.contains("Invalid document name")
            {
                Err((
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse {
                        success: false,
                        data: None,
                        error: Some(e),
                    }),
                ))
            } else {
                Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse {
                        success: false,
                        data: None,
                        error: Some("Failed to read debug file".to_string()),
                    }),
                ))
            }
        }
    }
}

/// Handler to delete debug output for a specific document
#[tracing::instrument(skip_all)]
async fn delete_debug_handler(
    Path(doc_name): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiResponse<()>>)> {
    match delete_debug_file(&doc_name) {
        Ok(()) => Ok(Json(ApiResponse {
            success: true,
            data: Some("Debug file deleted successfully"),
            error: None,
        })),
        Err(e) => {
            if e.contains("Path traversal")
                || e.contains("Path separators")
                || e.contains("Invalid document name")
            {
                Err((
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse {
                        success: false,
                        data: None,
                        error: Some(e),
                    }),
                ))
            } else {
                Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse {
                        success: false,
                        data: None,
                        error: Some("Failed to delete debug file".to_string()),
                    }),
                ))
            }
        }
    }
}

/// Handler to retrieve formula images
#[tracing::instrument(skip_all)]
async fn get_image_handler(
    Path((job_id, filename)): Path<(String, String)>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiResponse<()>>)> {
    if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse {
                success: false,
                data: None,
                error: Some("Invalid filename: path traversal not allowed".to_string()),
            }),
        ));
    }

    if !filename.ends_with(".png") {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse {
                success: false,
                data: None,
                error: Some("Invalid filename: only PNG files are allowed".to_string()),
            }),
        ));
    }

    let image_path = PathBuf::from(format!("/tmp/ferrules-api/{}/figures/{}", job_id, filename));

    match tokio::fs::read(&image_path).await {
        Ok(data) => Ok(Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, "image/png")
            .body(Body::from(data))
            .unwrap()),
        Err(_) => Err((
            StatusCode::NOT_FOUND,
            Json(ApiResponse {
                success: false,
                data: None,
                error: Some("Image not found".to_string()),
            }),
        )),
    }
}

fn update_formula_img_paths(doc: &mut ferrules_core::entities::ParsedDocument, job_id: &str) {
    use ferrules_core::blocks::BlockType;

    for block in &mut doc.blocks {
        if let BlockType::Formula(ref mut formula) = block.kind {
            if let Some(ref path) = formula.formula_img {
                if let Some(filename) = path.split('/').next_back() {
                    formula.formula_img = Some(format!("/images/{}/figures/{}", job_id, filename));
                }
            }
        }
    }
}

fn cleanup_old_image_dirs(hours: u64) -> anyhow::Result<()> {
    let base_dir = PathBuf::from("/tmp/ferrules-api");
    if !base_dir.exists() {
        return Ok(());
    }

    let now = std::time::SystemTime::now();
    let cutoff = std::time::Duration::from_secs(hours * 3600);

    for entry in std::fs::read_dir(&base_dir)? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        if let Ok(metadata) = entry.metadata() {
            if let Ok(modified) = metadata.modified() {
                if let Ok(age) = now.duration_since(modified) {
                    if age > cutoff {
                        if let Err(e) = std::fs::remove_dir_all(&path) {
                            tracing::warn!(
                                "Failed to remove old image directory {:?}: {}",
                                path,
                                e
                            );
                        } else {
                            tracing::info!("Removed old image directory: {:?}", path);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Initialize debug configuration
    let debug_output = args
        .debug_output
        .parse::<DebugOutput>()
        .unwrap_or_else(|e| {
            eprintln!("Warning: {e}");
            DebugOutput::NONE
        });
    init_debug_config(debug_output);

    // Check providers
    let providers = parse_ep_args(&args);
    // Initialize Sentry if DSN is provided
    let use_sentry = args.sentry_dsn.is_some();
    let _guard = if let Some(dsn) = args.sentry_dsn {
        Some(sentry::init((
            dsn,
            sentry::ClientOptions {
                release: sentry::release_name!(),
                traces_sample_rate: 1f32,
                sample_rate: 1f32,
                environment: Some(args.sentry_environment.into()),
                ..Default::default()
            },
        )))
    } else {
        None
    };

    init_tracing(
        Some(&args.otlp_endpoint),
        "ferrules-api".into(),
        false,
        use_sentry,
    )
    .expect("can't setup tracing for API");

    // Initialize correction engine
    if let Err(e) = initialize_for_cli() {
        tracing::error!("Failed to initialize text correction engine: {}", e);
        eprintln!("❌ Failed to initialize text correction engine: {e}");
        std::process::exit(1);
    }
    tracing::info!("📝 Text correction engine initialized successfully");

    let ort_config = ORTConfig {
        execution_providers: providers,
        intra_threads: args.intra_threads,
        inter_threads: args.inter_threads,
        opt_level: args.graph_opt_level.map(|v| v.try_into().unwrap()),
    };
    // Initialize the layout model and queues
    let parser = FerrulesParser::new(ort_config);
    let job_manager = JobManager::new();

    let app_state = AppState {
        parser,
        job_manager,
    };

    // Build our application with a route
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/parse", post(parse_document_handler))
        .route("/parse/sse", post(parse_document_sse_handler))
        .route("/parse/cancel/:job_id", post(cancel_job_handler))
        .route("/debug/:doc_name", get(get_debug_handler))
        .route("/debug/:doc_name", delete(delete_debug_handler))
        .route("/images/:job_id/figures/:filename", get(get_image_handler))
        .with_state(app_state)
        .layer(OtelAxumLayer::default())
        .layer(DefaultBodyLimit::max(MAX_SIZE_LIMIT));

    // Start background task for cleanup (debug files and image directories)
    tokio::spawn(async {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600)); // Run every hour
        loop {
            interval.tick().await;

            if let Err(e) = cleanup_old_debug_files(4) {
                tracing::warn!("Failed to cleanup old debug files: {}", e);
            } else {
                tracing::debug!("Debug file cleanup completed");
            }

            if let Err(e) = cleanup_old_image_dirs(1) {
                tracing::warn!("Failed to cleanup old image directories: {}", e);
            } else {
                tracing::debug!("Image directory cleanup completed");
            }
        }
    });

    // Run it
    let listener = TcpListener::bind(&args.listen_addr).await.unwrap();
    tracing::info!(
        "Starting ferrules service listening on {}",
        listener.local_addr().unwrap()
    );
    axum::serve(listener, app).await.unwrap();
}

#[tracing::instrument(skip_all)]
async fn health_check() -> impl IntoResponse {
    Json(ApiResponse {
        success: true,
        data: Some("Service is healthy"),
        error: None,
    })
}

#[tracing::instrument(skip_all)]
async fn parse_document_handler(
    headers: HeaderMap,
    state: State<AppState>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiResponse<()>>)> {
    // Extract the file from multipart form

    let mut temp_file = NamedTempFile::new().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse {
                success: false,
                data: None,
                error: Some(format!("Failed to create temp file: {e}")),
            }),
        )
    })?;

    let mut options = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse {
                success: false,
                data: None,
                error: Some(format!("Failed to get next field: {e}")),
            }),
        )
    })? {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "file" => {
                // Stream the field data to the temp file
                let mut field_stream = field;
                while let Some(chunk) = field_stream.chunk().await.map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(ApiResponse {
                            success: false,
                            data: None,
                            error: Some(format!("Failed to read chunk: {e}")),
                        }),
                    )
                })? {
                    temp_file.write_all(&chunk).map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ApiResponse {
                                success: false,
                                data: None,
                                error: Some(format!("Failed to write to temp file: {e}")),
                            }),
                        )
                    })?;
                }
                temp_file.flush().map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ApiResponse {
                            success: false,
                            data: None,
                            error: Some(format!("Failed to flush temp file: {e}")),
                        }),
                    )
                })?;
                temp_file.seek(std::io::SeekFrom::Start(0)).map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ApiResponse {
                            success: false,
                            data: None,
                            error: Some(format!("Failed to seek temp file: {e}")),
                        }),
                    )
                })?;
            }
            "options" => {
                let options_str = field.text().await.map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(ApiResponse {
                            success: false,
                            data: None,
                            error: Some(format!("Failed to read options: {e}")),
                        }),
                    )
                })?;
                options = Some(serde_json::from_str::<ParseOptions>(&options_str).map_err(
                    |e| {
                        (
                            StatusCode::BAD_REQUEST,
                            Json(ApiResponse {
                                success: false,
                                data: None,
                                error: Some(format!("Failed to parse options: {e}")),
                            }),
                        )
                    },
                )?);
            }
            _ => continue,
        }
    }

    let file = File::open(temp_file.path()).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse {
                success: false,
                data: None,
                error: Some(format!("Failed to open temp file: {e}")),
            }),
        )
    })?;

    let mmap = unsafe {
        Mmap::map(&file).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    error: Some(format!("Failed to memory map file: {e}")),
                }),
            )
        })?
    };
    let page_range = if let Some(options) = options {
        if let Some(range_str) = options.page_range {
            Some(parse_page_range(&range_str).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse {
                        success: false,
                        data: None,
                        error: Some(e.to_string()),
                    }),
                )
            })?)
        } else {
            None
        }
    } else {
        None
    };

    let config = FerrulesParseConfig {
        password: None,
        flatten_pdf: true,
        page_range,
        debug_dir: None,
    };

    // Generate doc_name and set debug context
    let doc_name = Uuid::new_v4().to_string();
    set_debug_context(doc_name.clone(), None);

    let mut doc = state
        .parser
        .parse_document(
            &mmap,
            doc_name.clone(),
            config,
            Some(|_| {}),
            None::<fn() -> bool>,
        )
        .await
        .map_err(|e| {
            clear_debug_context(); // Clear on error
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    error: Some(e.to_string()),
                }),
            )
        })?;

    // Save formula images to /tmp/ferrules-api/{job_id}/figures/
    let figures_dir = PathBuf::from(format!("/tmp/ferrules-api/{}/figures", doc_name));
    std::fs::create_dir_all(&figures_dir).ok();
    if let Err(e) = save_doc_images(&figures_dir, &doc) {
        tracing::warn!("Failed to save formula images: {}", e);
    }

    // Update formula_img paths to full API paths
    update_formula_img_paths(&mut doc, &doc_name);

    let accept_header = headers.get(ACCEPT).and_then(|h| h.to_str().ok());

    let result = match accept_header {
        Some("text/markdown") => {
            let markdown = to_markdown(&doc, &doc.doc_name, None).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse {
                        success: false,
                        data: None,
                        error: Some(format!("Failed to convert to markdown: {e}")),
                    }),
                )
            })?;

            Ok(Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_TYPE, "text/markdown")
                .body::<String>(markdown)
                .unwrap())
        }
        _ => {
            // NOTE: Default to JSON
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_TYPE, "application/json")
                .body(
                    // Use to_vec_pretty + from_utf8 to preserve Unicode characters and format prettily
                    String::from_utf8(
                        serde_json::to_vec_pretty(&ApiResponse {
                            success: true,
                            data: Some(doc),
                            error: None,
                        })
                        .unwrap(),
                    )
                    .unwrap(),
                )
                .unwrap())
        }
    };

    // Clear debug context when done
    clear_debug_context();

    result
}

fn parse_page_range(range_str: &str) -> anyhow::Result<std::ops::Range<usize>> {
    if let Some((start, end)) = range_str.split_once('-') {
        let start: usize = start.trim().parse()?;
        let end: usize = end.trim().parse()?;
        if start > 0 && end >= start {
            Ok(std::ops::Range {
                start: start - 1,
                end,
            })
        } else {
            anyhow::bail!("Invalid page range: start must be > 0 and end must be >= start")
        }
    } else {
        // Single page
        let page: usize = range_str.trim().parse()?;
        if page > 0 {
            Ok(std::ops::Range {
                start: page - 1,
                end: page,
            })
        } else {
            anyhow::bail!("Page number must be greater than 0")
        }
    }
}

#[tracing::instrument(skip_all)]
async fn parse_document_sse_handler(
    _headers: HeaderMap,
    state: State<AppState>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiResponse<()>>)> {
    // Create a channel for sending events
    let (tx, rx) = mpsc::channel::<ParseEvent>(32);

    // Generate job ID and start job tracking
    let job_id = Uuid::new_v4();
    let cancellation_token = state.job_manager.start_job(job_id, tx.clone()).await;

    // Extract the file from multipart form (same as regular handler)
    let mut temp_file = NamedTempFile::new().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse {
                success: false,
                data: None,
                error: Some(format!("Failed to create temp file: {e}")),
            }),
        )
    })?;

    let mut options = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse {
                success: false,
                data: None,
                error: Some(format!("Failed to get next field: {e}")),
            }),
        )
    })? {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "file" => {
                let mut field_stream = field;
                while let Some(chunk) = field_stream.chunk().await.map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(ApiResponse {
                            success: false,
                            data: None,
                            error: Some(format!("Failed to read chunk: {e}")),
                        }),
                    )
                })? {
                    temp_file.write_all(&chunk).map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ApiResponse {
                                success: false,
                                data: None,
                                error: Some(format!("Failed to write to temp file: {e}")),
                            }),
                        )
                    })?;
                }
                temp_file.flush().map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ApiResponse {
                            success: false,
                            data: None,
                            error: Some(format!("Failed to flush temp file: {e}")),
                        }),
                    )
                })?;
                temp_file.seek(std::io::SeekFrom::Start(0)).map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ApiResponse {
                            success: false,
                            data: None,
                            error: Some(format!("Failed to seek temp file: {e}")),
                        }),
                    )
                })?;
            }
            "options" => {
                let options_str = field.text().await.map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(ApiResponse {
                            success: false,
                            data: None,
                            error: Some(format!("Failed to read options: {e}")),
                        }),
                    )
                })?;
                options = Some(serde_json::from_str::<ParseOptions>(&options_str).map_err(
                    |e| {
                        (
                            StatusCode::BAD_REQUEST,
                            Json(ApiResponse {
                                success: false,
                                data: None,
                                error: Some(format!("Failed to parse options: {e}")),
                            }),
                        )
                    },
                )?);
            }
            _ => continue,
        }
    }

    // Parse page range
    let page_range = if let Some(options) = options {
        if let Some(range_str) = options.page_range {
            Some(parse_page_range(&range_str).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse {
                        success: false,
                        data: None,
                        error: Some(e.to_string()),
                    }),
                )
            })?)
        } else {
            None
        }
    } else {
        None
    };

    // Create memory map before spawning task
    let file = File::open(temp_file.path()).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse {
                success: false,
                data: None,
                error: Some(format!("Failed to open temp file: {e}")),
            }),
        )
    })?;

    let mmap = unsafe {
        Mmap::map(&file).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    error: Some(format!("Failed to memory map file: {e}")),
                }),
            )
        })?
    };

    // Send job started event
    let _ = tx.send(ParseEvent::JobStarted { job_id }).await;

    // Spawn parsing task
    let tx_clone = tx.clone();
    let parser = state.parser.clone();
    let job_manager = state.job_manager.clone();
    let cancellation_token_clone = cancellation_token.clone();

    tokio::spawn(async move {
        // Keep the temp file alive by moving it into the task
        let _temp_file = temp_file; // Keep alive until end of task

        let config = FerrulesParseConfig {
            password: None,
            flatten_pdf: true,
            page_range,
            debug_dir: None,
        };

        // Check for cancellation before starting
        if cancellation_token_clone.is_cancelled() {
            job_manager.complete_job(job_id).await;
            return;
        }

        // Get page count using the fixed method
        let total_pages = match parser.get_page_count(&mmap, config.password).await {
            Ok(count) => count,
            Err(e) => {
                let _ = tx_clone
                    .send(ParseEvent::Error {
                        message: format!("Failed to get page count: {e}"),
                    })
                    .await;
                job_manager.complete_job(job_id).await;
                return;
            }
        };

        let pages_completed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let tx_progress = tx_clone.clone();
        let pages_completed_clone = pages_completed.clone();

        // Create progress callback
        let progress_callback = {
            let tx_progress = tx_progress.clone();
            move |page_id| {
                let completed =
                    pages_completed_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                tracing::info!(
                    "Progress callback called for page {} (completed: {}/{})",
                    page_id,
                    completed,
                    total_pages
                );
                let _ = tx_progress.try_send(ParseEvent::Progress {
                    pages_completed: completed,
                    total_pages,
                    page_id,
                });
            }
        };

        // Create cancellation callback
        let cancellation_callback = {
            let token = cancellation_token_clone.clone();
            move || {
                let is_cancelled = token.is_cancelled();
                if is_cancelled {
                    tracing::info!("Cancellation callback detected cancellation!");
                }
                is_cancelled
            }
        };

        // Parse document with cancellation callback - much simpler!
        let result = parser
            .parse_document(
                &mmap,
                job_id.to_string(),
                config,
                Some(progress_callback),
                Some(cancellation_callback),
            )
            .await;

        match result {
            Ok(mut doc) => {
                if !cancellation_token_clone.is_cancelled() {
                    // Save formula images to /tmp/ferrules-api/{job_id}/figures/
                    let figures_dir =
                        PathBuf::from(format!("/tmp/ferrules-api/{}/figures", job_id));
                    let _ = std::fs::create_dir_all(&figures_dir);
                    if let Err(e) = save_doc_images(&figures_dir, &doc) {
                        tracing::warn!("Failed to save formula images: {}", e);
                    }

                    // Update formula_img paths to full API paths
                    update_formula_img_paths(&mut doc, &job_id.to_string());

                    let _ = tx_clone
                        .send(ParseEvent::Complete {
                            document: serde_json::to_value(&doc).unwrap_or_default(),
                            total_pages: doc.pages.len(),
                        })
                        .await;
                }
            }
            Err(e) => {
                // Check if the error is due to cancellation
                if e.to_string().contains("cancelled") {
                    tracing::info!("Document processing was cancelled: {}", e);
                    let _ = tx_clone
                        .send(ParseEvent::Cancelled {
                            message: "Processing was cancelled".to_string(),
                        })
                        .await;
                } else if !cancellation_token_clone.is_cancelled() {
                    let _ = tx_clone
                        .send(ParseEvent::Error {
                            message: e.to_string(),
                        })
                        .await;
                }
            }
        }

        // Clean up job when done
        job_manager.complete_job(job_id).await;
    });

    // Create SSE stream
    let stream = ReceiverStream::new(rx).map(|event| {
        // Use to_vec_pretty + from_utf8 to preserve Unicode characters and format prettily
        let data = String::from_utf8(serde_json::to_vec_pretty(&event).unwrap_or_default())
            .unwrap_or_default();
        Ok::<_, std::convert::Infallible>(
            axum::response::sse::Event::default()
                .event(match &event {
                    ParseEvent::JobStarted { .. } => "job_started",
                    ParseEvent::Progress { .. } => "progress",
                    ParseEvent::Complete { .. } => "complete",
                    ParseEvent::Cancelled { .. } => "cancelled",
                    ParseEvent::Error { .. } => "error",
                })
                .data(data),
        )
    });

    Ok(Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(30))
            .text("keep-alive-text"),
    ))
}

#[tracing::instrument(skip_all)]
async fn cancel_job_handler(
    Path(job_id): Path<Uuid>,
    State(app_state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiResponse<()>>)> {
    match app_state.job_manager.cancel_job(job_id).await {
        Ok(()) => Ok(Json(ApiResponse {
            success: true,
            data: Some("Job cancelled successfully"),
            error: None,
        })),
        Err(error_msg) => Err((
            StatusCode::NOT_FOUND,
            Json(ApiResponse {
                success: false,
                data: None,
                error: Some(error_msg),
            }),
        )),
    }
}
