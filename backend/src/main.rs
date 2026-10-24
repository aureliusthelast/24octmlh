mod auditor;
mod parser;
mod pdf_scanner;

use axum::{
    body::Bytes,
    extract::Multipart,
    http::{HeaderMap, Method, StatusCode},
    response::Json,
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use std::path::PathBuf;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "invoice_auditor_backend=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any);

    // Resolve the frontend dist directory whether run from repo root or backend/ subdir
    let frontend_dist = std::env::var("FRONTEND_DIST")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            if PathBuf::from("frontend/dist").exists() {
                PathBuf::from("frontend/dist")
            } else {
                PathBuf::from("../frontend/dist")
            }
        });

    // API routes
    let api_routes = Router::new()
        .route("/health", get(health_check))
        .route("/api/audit-json", post(audit_json_handler))
        .route("/api/upload", post(upload_handler))
        .layer(cors);

    // Build final app: API routes first, then fall back to serving frontend static files
    let app = if frontend_dist.exists() {
        tracing::info!("Serving frontend from: {}", frontend_dist.display());
        let serve_dir = ServeDir::new(&frontend_dist)
            .not_found_service(ServeFile::new(frontend_dist.join("index.html")));
        api_routes.fallback_service(serve_dir)
    } else {
        tracing::warn!(
            "Frontend dist not found at '{}'. Run 'cd frontend && npm run build' to enable serving the UI from the backend.",
            frontend_dist.display()
        );
        api_routes
    };

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Starting Invoice Auditor Backend on http://{}", addr);
    if frontend_dist.exists() {
        tracing::info!("Open http://localhost:{} in your browser (works on all devices on the network!)", port);
    }

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => {
            eprintln!("\n❌ Error: Port {} is already in use by another process.", port);
            eprintln!("💡 Tip: Free port {} with: fuser -k {}/tcp (or specify another port: PORT=3001 cargo run)\n", port, port);
            std::process::exit(1);
        }
        Err(err) => {
            eprintln!("❌ Failed to bind to address {}: {}", addr, err);
            std::process::exit(1);
        }
    };

    axum::serve(listener, app)
        .await
        .expect("Server failed during execution");
}

async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": "invoice-auditor",
        "version": "0.2.0",
        "features": {
            "gemma_pdf_scanner": true,
            "resilient_json_parser": true
        }
    }))
}

async fn audit_json_handler(
    body_bytes: Bytes,
) -> Result<Json<auditor::AuditReport>, (StatusCode, Json<serde_json::Value>)> {
    let text = String::from_utf8(body_bytes.to_vec()).map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("Payload is not valid UTF-8 text: {}", err)
            })),
        )
    })?;

    let invoice = parser::parse_flexible_invoice(&text).map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("Failed to parse invoice JSON: {}", err),
                "hint": "Check payload structure or format."
            })),
        )
    })?;

    tracing::info!("Audited flexible JSON invoice id: {}", invoice.invoice_id);
    let report = auditor::audit_invoice(&invoice);
    Ok(Json(report))
}

async fn upload_handler(
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<auditor::AuditReport>, (StatusCode, Json<serde_json::Value>)> {
    let mut file_content: Option<Vec<u8>> = None;
    let mut file_name = String::from("uploaded_document");
    let mut form_api_key: Option<String> = None;
    let mut form_ollama_host: Option<String> = None;

    // Check custom request headers for AI configuration
    let header_api_key = headers
        .get("x-gemini-api-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let header_ollama = headers
        .get("x-gemma-endpoint")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    while let Ok(Some(field)) = multipart.next_field().await {
        let field_name = field.name().unwrap_or_default().to_string();

        if field_name == "api_key" || field_name == "apiKey" {
            if let Ok(text) = field.text().await {
                if !text.trim().is_empty() {
                    form_api_key = Some(text.trim().to_string());
                }
            }
        } else if field_name == "ollama_host" || field_name == "ollamaHost" {
            if let Ok(text) = field.text().await {
                if !text.trim().is_empty() {
                    form_ollama_host = Some(text.trim().to_string());
                }
            }
        } else if let Some(fname) = field.file_name() {
            file_name = fname.to_string();
            if let Ok(data) = field.bytes().await {
                file_content = Some(data.to_vec());
            }
        } else if file_content.is_none() {
            if let Ok(data) = field.bytes().await {
                if !data.is_empty() {
                    file_content = Some(data.to_vec());
                }
            }
        }
    }

    let bytes = match file_content {
        Some(b) if !b.is_empty() => b,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "No file received in multipart request or file was empty"
                })),
            ));
        }
    };

    let resolved_api_key = header_api_key.or(form_api_key);
    let resolved_ollama = header_ollama.or(form_ollama_host);

    let is_pdf_file = pdf_scanner::is_pdf(&bytes, &file_name);

    let invoice = if is_pdf_file {
        tracing::info!("Received PDF file '{}' ({} bytes). Running PDF extraction...", file_name, bytes.len());
        pdf_scanner::process_pdf(&bytes, resolved_api_key.as_deref(), resolved_ollama.as_deref())
            .await
            .map_err(|err| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": format!("Failed extracting PDF invoice '{}': {}", file_name, err),
                        "hint": "Ensure PDF contains recognizable text or set GEMINI_API_KEY for vision OCR on scanned images."
                    })),
                )
            })?
    } else {
        tracing::info!("Received text/JSON file '{}' ({} bytes). Normalizing and parsing...", file_name, bytes.len());
        let text = String::from_utf8(bytes).map_err(|err| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Uploaded file '{}' is not valid UTF-8 text: {}", file_name, err)
                })),
            )
        })?;

        parser::parse_flexible_invoice(&text).map_err(|err| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Failed to parse JSON file '{}': {}", file_name, err),
                    "hint": "Check that the file contains invoice data."
                })),
            )
        })?
    };

    tracing::info!("Successfully parsed document for invoice ID: {}", invoice.invoice_id);
    let report = auditor::audit_invoice(&invoice);
    Ok(Json(report))
}
