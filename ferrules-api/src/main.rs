use axum::{
    extract::{DefaultBodyLimit, Multipart, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use axum_tracing_opentelemetry::middleware::OtelAxumLayer;
use ferrules_api::init_tracing;
use ferrules_core::{
    layout::{model::ORTLayoutParser, ParseLayoutQueue},
    parse::{document::parse_document, native::ParseNativeQueue},
};
use memmap2::Mmap;
use serde::{Deserialize, Serialize};
use std::{
    io::{Seek, Write},
    sync::Arc,
};
use tempfile::NamedTempFile;
use tokio::{fs::File, net::TcpListener};
use uuid::Uuid;

const MAX_SIZE_LIMIT: usize = 250 * 1024 * 1024;

#[derive(Debug, Serialize)]
struct ApiResponse<T> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ParseOptions {
    page_range: Option<String>,
    _save_images: Option<bool>,
}

#[derive(Clone)]
struct AppState {
    layout_queue: ParseLayoutQueue,
    native_queue: ParseNativeQueue,
}

#[tokio::main]
async fn main() {
    init_tracing("http://localhost:4317", "ferrules-api".into(), false)
        .expect("can't setup tracing for API");

    // Initialize the layout model and queues
    let layout_model = Arc::new(ORTLayoutParser::new().expect("Failed to load layout model"));
    let layout_queue = ParseLayoutQueue::new(layout_model);
    let native_queue = ParseNativeQueue::new();

    let app_state = AppState {
        layout_queue,
        native_queue,
    };

    // Build our application with a route
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/parse", post(parse_document_handler))
        .with_state(app_state)
        .layer(OtelAxumLayer::default())
        .layer(DefaultBodyLimit::max(MAX_SIZE_LIMIT));

    // Run it
    let listener = TcpListener::bind("0.0.0.0:3002").await.unwrap();
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
                error: Some(format!("Failed to create temp file: {}", e)),
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
                error: Some(format!("Failed to get next field: {}", e)),
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
                            error: Some(format!("Failed to read chunk: {}", e)),
                        }),
                    )
                })? {
                    temp_file.write_all(&chunk).map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ApiResponse {
                                success: false,
                                data: None,
                                error: Some(format!("Failed to write to temp file: {}", e)),
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
                            error: Some(format!("Failed to flush temp file: {}", e)),
                        }),
                    )
                })?;
                temp_file.seek(std::io::SeekFrom::Start(0)).map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ApiResponse {
                            success: false,
                            data: None,
                            error: Some(format!("Failed to seek temp file: {}", e)),
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
                            error: Some(format!("Failed to read options: {}", e)),
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
                                error: Some(format!("Failed to parse options: {}", e)),
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
                error: Some(format!("Failed to open temp file: {}", e)),
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
                    error: Some(format!("Failed to memory map file: {}", e)),
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

    let doc = parse_document(
        &mmap,
        Uuid::new_v4().to_string(),
        None,
        true,
        page_range,
        state.layout_queue.clone(),
        state.native_queue.clone(),
        false,
        Some(|_| {}),
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse {
                success: false,
                data: None,
                error: Some(e.to_string()),
            }),
        )
    })?;

    Ok(Json(ApiResponse {
        success: true,
        data: Some(doc),
        error: None,
    }))
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
