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
    #[arg(long, env = "OTLP_ENDPOINT")]
    otlp_endpoint: Option<String>,

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

    /// Restrict the ANE table-transformer (ane-b4) to CPUAndNeuralEngine
    /// compute units. Enabled by default — the ANE-b4 model was trained for
    /// this path; routing to GPU shifts FP16 scores and regresses table
    /// detection. The layout model is intentionally unrestricted (all compute
    /// units) to allow GPU routing, which is faster.
    #[arg(
        long,
        env = "FERRULES_COREML_ANE_ONLY_TABLE_ANE",
        default_value_t = false,
        help = "Restrict ANE table-transformer to CPUAndNeuralEngine (off by default — all compute units is faster and quality-equivalent for NeuralNetwork format)"
    )]
    pub coreml_ane_only_table_ane: bool,

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

    /// Enable profiling for layout model
    #[arg(long, help = "Enable profiling for the layout model (saved as .json)")]
    profile_layout: bool,

    /// Enable profiling for table transformer model
    #[arg(
        long,
        help = "Enable profiling for the table transformer model (saved as .json)"
    )]
    profile_table: bool,

    /// Directory to cache compiled CoreML models (persists across restarts).
    /// Pass "none" or "disable" to skip caching entirely. When unset, defaults
    /// to `$XDG_CACHE_HOME/ferrules/coreml` (or the platform equivalent).
    #[arg(long, env = "FERRULES_COREML_CACHE_DIR")]
    model_cache_dir: Option<String>,

    /// Use CoreML's MLProgram model format instead of the default
    /// NeuralNetwork. Off by default — MLProgram changes YOLOv8 layout
    /// detection outputs (different FP16 path), requiring per-document
    /// quality validation before enabling in production.
    #[arg(long, env = "FERRULES_COREML_MLPROGRAM")]
    coreml_mlprogram: bool,

    /// Log per-node CoreML vs CPU coverage to stderr at session creation.
    /// Diagnostic only.
    #[arg(long, env = "FERRULES_COREML_PROFILE_COMPUTE_PLAN")]
    coreml_profile_compute_plan: bool,

    /// Request CoreML's `FastPrediction` specialization for the layout ONNX
    /// session. Trades longer cold-start compile time + larger
    /// `.mlmodelc` on disk for lower inference latency. Only meaningful when
    /// a model cache dir is set (otherwise paid on every start). A/B test
    /// before enabling in production.
    #[arg(long, env = "FERRULES_COREML_FAST_PREDICTION_LAYOUT")]
    coreml_fast_prediction_layout: bool,

    /// Request `FastPrediction` for the standard (CPU + GPU)
    /// table-transformer session.
    #[arg(long, env = "FERRULES_COREML_FAST_PREDICTION_TABLE")]
    coreml_fast_prediction_table: bool,

    /// Request `FastPrediction` for the ANE-only table-transformer session.
    #[arg(long, env = "FERRULES_COREML_FAST_PREDICTION_TABLE_ANE")]
    coreml_fast_prediction_table_ane: bool,

    /// Declare that the layout ONNX (`yolov8s-doclaynet`, input
    /// `[1, 3, 1024, 1024]`) has fully-static input shapes so CoreML can
    /// skip per-call shape specialization. Safe for this model; do not add
    /// an equivalent flag for graphs with dynamic dims. Defaults to true.
    #[arg(long, env = "FERRULES_COREML_STATIC_INPUT_SHAPES_LAYOUT", default_value_t = true, action = clap::ArgAction::Set)]
    coreml_static_input_shapes_layout: bool,

    /// Declare that the ANE table-transformer ONNX (input
    /// `[4, 3, 1000, 1000]`) has fully-static input shapes. The standard
    /// (fp16) table-transformer has dynamic input dims and intentionally
    /// has no corresponding flag.
    #[arg(long, env = "FERRULES_COREML_STATIC_INPUT_SHAPES_TABLE_ANE")]
    coreml_static_input_shapes_table_ane: bool,
}

/// Resolve the effective CoreML model cache root. Returns `None` if the user
/// explicitly disabled caching or if no default location can be determined.
fn resolve_coreml_cache_dir(raw: Option<&str>) -> Option<PathBuf> {
    if let Some(value) = raw {
        let trimmed = value.trim();
        if trimmed.is_empty()
            || trimmed.eq_ignore_ascii_case("none")
            || trimmed.eq_ignore_ascii_case("disable")
            || trimmed.eq_ignore_ascii_case("off")
        {
            return None;
        }
        return Some(PathBuf::from(trimmed));
    }
    let base = dirs::cache_dir()?;
    Some(base.join("ferrules").join("coreml"))
}

/// Check if an error is GPU-related and exit the process if so.
/// On restart, docker-init.sh will re-detect GPU availability and fall back to CPU if needed.
fn check_gpu_error_and_exit(error: &dyn std::fmt::Display) {
    let err_msg = error.to_string().to_lowercase();
    if err_msg.contains("cuda") || err_msg.contains("gpu") || err_msg.contains("device") {
        tracing::error!("GPU error detected: {}", error);
        tracing::error!("Exiting process for restart with CPU fallback");
        std::process::exit(1);
    }
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
        providers.push(OrtExecutionProvider::CoreML { ane_only: false });
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
    JobStarted {
        job_id: Uuid,
        total_pages: usize,
        title: Option<String>,
    },
    #[serde(rename = "progress")]
    Progress {
        pages_completed: usize,
        total_pages: usize,
        page_id: usize,
    },
    #[serde(rename = "complete")]
    Complete {
        /// URL to fetch the document JSON (instead of embedding 40MB+ in SSE)
        document_url: String,
        markdown_url: String,
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

/// Handler to retrieve markdown output
#[tracing::instrument(skip_all)]
async fn get_markdown_handler(
    Path(job_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiResponse<()>>)> {
    if job_id.contains("..") || job_id.contains('/') || job_id.contains('\\') {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse {
                success: false,
                data: None,
                error: Some("Invalid job_id: path traversal not allowed".to_string()),
            }),
        ));
    }

    let markdown_path = PathBuf::from(format!("/tmp/ferrules-api/{}/raw.md", job_id));

    match tokio::fs::read_to_string(&markdown_path).await {
        Ok(content) => Ok(Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, "text/markdown; charset=utf-8")
            .body(Body::from(content))
            .unwrap()),
        Err(_) => Err((
            StatusCode::NOT_FOUND,
            Json(ApiResponse {
                success: false,
                data: None,
                error: Some("Markdown file not found".to_string()),
            }),
        )),
    }
}

/// Handler to serve document JSON from disk
async fn get_document_handler(
    Path(job_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiResponse<()>>)> {
    if job_id.contains("..") || job_id.contains('/') || job_id.contains('\\') {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse {
                success: false,
                data: None,
                error: Some("Invalid job_id: path traversal not allowed".to_string()),
            }),
        ));
    }

    let document_path = PathBuf::from(format!("/tmp/ferrules-api/{}/document.json", job_id));

    match tokio::fs::read(&document_path).await {
        Ok(content) => Ok(Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, "application/json; charset=utf-8")
            .body(Body::from(content))
            .unwrap()),
        Err(_) => Err((
            StatusCode::NOT_FOUND,
            Json(ApiResponse {
                success: false,
                data: None,
                error: Some("Document file not found".to_string()),
            }),
        )),
    }
}

fn update_image_paths(doc: &mut ferrules_core::entities::ParsedDocument, job_id: &str) {
    use ferrules_core::blocks::BlockType;

    for block in &mut doc.blocks {
        match &mut block.kind {
            BlockType::Formula(ref mut formula) => {
                if let Some(ref path) = formula.formula_img {
                    if let Some(filename) = path.split('/').next_back() {
                        formula.formula_img =
                            Some(format!("/images/{}/figures/{}", job_id, filename));
                    }
                }
            }
            BlockType::Image(ref mut image) => {
                image.image_path = Some(format!("/images/{}/figures/img_{}.png", job_id, image.id));
            }
            BlockType::Figure(ref mut figure) => {
                figure.image_path =
                    Some(format!("/images/{}/figures/fig_{}.png", job_id, figure.id));
            }
            _ => {}
        }
    }
}

fn cleanup_old_job_dirs(hours: u64) -> anyhow::Result<()> {
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
                            tracing::warn!("Failed to remove old job directory {:?}: {}", path, e);
                        } else {
                            tracing::info!("Removed old job directory: {:?}", path);
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
        args.otlp_endpoint.as_deref(),
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

    // Initialize Prometheus exporter
    let builder = metrics_exporter_prometheus::PrometheusBuilder::new()
        .set_buckets_for_metric(
            metrics_exporter_prometheus::Matcher::Suffix("_ms".to_string()),
            &[
                0.0, 1.0, 2.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0,
                90.0, 100.0, 125.0, 150.0, 175.0, 200.0, 250.0, 300.0, 350.0, 400.0, 450.0, 500.0,
                600.0, 700.0, 800.0, 900.0, 1000.0, 1250.0, 1500.0, 1750.0, 2000.0, 2500.0, 3000.0,
                3500.0, 4000.0, 4500.0, 5000.0, 6000.0, 7000.0, 8000.0, 9000.0, 10000.0, 15000.0,
                20000.0, 30000.0, 45000.0, 60000.0, 90000.0, 120000.0, 180000.0, 240000.0,
                300000.0,
            ],
        )
        .expect("failed to set buckets");
    let handle = builder
        .install_recorder()
        .expect("failed to install Prometheus recorder");

    let model_cache_dir = resolve_coreml_cache_dir(args.model_cache_dir.as_deref());
    if let Some(ref dir) = model_cache_dir {
        tracing::info!("CoreML model cache directory: {:?}", dir);
    } else {
        tracing::info!("CoreML model cache disabled; models will recompile on each start");
    }

    let ort_config = ORTConfig {
        execution_providers: providers,
        intra_threads: args.intra_threads,
        inter_threads: args.inter_threads,
        opt_level: args.graph_opt_level.map(|v| v.try_into().unwrap()),
        warmup: true,
        profile_layout: if args.profile_layout {
            Some(std::path::PathBuf::from("profile_layout_api"))
        } else {
            None
        },
        profile_table: if args.profile_table {
            Some(std::path::PathBuf::from("profile_table_api"))
        } else {
            None
        },
        model_cache_dir,
        coreml_mlprogram: args.coreml_mlprogram,
        coreml_ane_only_layout: false,
        coreml_ane_only_table_ane: args.coreml_ane_only_table_ane,
        coreml_profile_compute_plan: args.coreml_profile_compute_plan,
        coreml_fast_prediction_layout: args.coreml_fast_prediction_layout,
        coreml_fast_prediction_table: args.coreml_fast_prediction_table,
        coreml_fast_prediction_table_ane: args.coreml_fast_prediction_table_ane,
        coreml_static_input_shapes_layout: args.coreml_static_input_shapes_layout,
        coreml_static_input_shapes_table_ane: args.coreml_static_input_shapes_table_ane,
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
        .route("/debug/queue-status", get(queue_status_handler))
        .route("/debug/:doc_name", get(get_debug_handler))
        .route("/debug/:doc_name", delete(delete_debug_handler))
        .route("/images/:job_id/figures/:filename", get(get_image_handler))
        .route("/markdown/:job_id", get(get_markdown_handler))
        .route("/document/:job_id", get(get_document_handler))
        .route("/metrics", get(move || std::future::ready(handle.render())))
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

            if let Err(e) = cleanup_old_job_dirs(1) {
                tracing::warn!("Failed to cleanup old job directories: {}", e);
            } else {
                tracing::debug!("Job directory cleanup completed");
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

async fn queue_status_handler(state: State<AppState>) -> impl IntoResponse {
    let status = state.parser.queue_status();
    Json(serde_json::json!({
        "native_thread_alive": status.native_thread_alive,
        "native_queue_capacity": status.native_queue_capacity,
        "native_queue_max_capacity": status.native_queue_max_capacity,
    }))
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
            check_gpu_error_and_exit(&e);
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
    update_image_paths(&mut doc, &doc_name);

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

        // Get PDF metadata (page count and title)
        let (total_pages, pdf_title) = match parser.get_pdf_metadata(&mmap, config.password).await {
            Ok(metadata) => (metadata.page_count, metadata.title),
            Err(e) => {
                let _ = tx_clone
                    .send(ParseEvent::Error {
                        message: format!("Failed to get PDF metadata: {e}"),
                    })
                    .await;
                job_manager.complete_job(job_id).await;
                return;
            }
        };

        // Send job started event with metadata
        let _ = tx_clone
            .send(ParseEvent::JobStarted {
                job_id,
                total_pages,
                title: pdf_title,
            })
            .await;

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

                // Log when all pages are parsed (merge_elements_into_blocks runs inside parse_document)
                if completed == total_pages {
                    tracing::info!("All pages parsed, merge_elements_into_blocks starting...");
                }
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

        // Set debug context for this document processing task
        set_debug_context(job_id.to_string(), None);

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
                    use std::time::Instant;
                    let post_start = Instant::now();

                    // Save formula images
                    let img_start = Instant::now();
                    let job_dir = PathBuf::from(format!("/tmp/ferrules-api/{}", job_id));
                    let figures_dir = job_dir.join("figures");
                    let _ = std::fs::create_dir_all(&figures_dir);
                    if let Err(e) = save_doc_images(&figures_dir, &doc) {
                        tracing::warn!("Failed to save formula images: {}", e);
                    }
                    tracing::info!("⏱️ save_doc_images took {:?}", img_start.elapsed());

                    // Generate markdown
                    let md_start = Instant::now();
                    let markdown_url = format!("/markdown/{}", job_id);
                    match to_markdown(&doc, &doc.doc_name, None) {
                        Ok(markdown) => {
                            let markdown_path = job_dir.join("raw.md");
                            if let Err(e) = std::fs::write(&markdown_path, &markdown) {
                                tracing::warn!("Failed to save markdown: {}", e);
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to generate markdown: {}", e);
                        }
                    }
                    tracing::info!("⏱️ to_markdown took {:?}", md_start.elapsed());

                    // Finalize (update paths + JSON serialization)
                    let path_start = Instant::now();
                    update_image_paths(&mut doc, &job_id.to_string());
                    tracing::info!("⏱️ update_image_paths took {:?}", path_start.elapsed());

                    // Serialize and save document JSON to disk (instead of sending 40MB+ via SSE)
                    let json_start = Instant::now();
                    let json_string = serde_json::to_string(&doc).unwrap_or_default();
                    let json_path = job_dir.join("document.json");
                    if let Err(e) = std::fs::write(&json_path, &json_string) {
                        tracing::warn!("Failed to save document JSON: {}", e);
                    }
                    tracing::info!(
                        "⏱️ JSON serialization + save took {:?} ({} bytes)",
                        json_start.elapsed(),
                        json_string.len()
                    );

                    tracing::info!(
                        "⏱️ Total API post-processing took {:?}",
                        post_start.elapsed()
                    );

                    // Send URL instead of full document to keep SSE lightweight
                    let document_url = format!("/document/{}", job_id);
                    let _ = tx_clone
                        .send(ParseEvent::Complete {
                            document_url,
                            markdown_url,
                            total_pages: doc.pages.len(),
                        })
                        .await;
                }
            }
            Err(e) => {
                check_gpu_error_and_exit(&e);
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

        // Clear debug context
        clear_debug_context();

        // Clean up job when done
        job_manager.complete_job(job_id).await;
    });

    // Create SSE stream
    let stream = ReceiverStream::new(rx).map(|event| {
        // Use to_string to preserve Unicode characters and produce single-line JSON for SSE
        let data = serde_json::to_string(&event).unwrap_or_default();
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
            .text(r#"{"type":"keep_alive"}"#),
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
