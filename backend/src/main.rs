mod auditor;

use axum::{
    extract::Multipart,
    http::{Method, StatusCode},
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

    // Resolve the frontend dist directory relative to the backend binary's working dir
    let frontend_dist = std::env::var("FRONTEND_DIST")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("../frontend/dist"));

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
        "version": "0.1.0"
    }))
}

async fn audit_json_handler(
    Json(invoice): Json<auditor::InvoiceData>,
) -> Result<Json<auditor::AuditReport>, (StatusCode, Json<serde_json::Value>)> {
    tracing::info!("Auditing JSON invoice id: {}", invoice.invoice_id);
    let report = auditor::audit_invoice(&invoice);
    Ok(Json(report))
}

async fn upload_handler(
    mut multipart: Multipart,
) -> Result<Json<auditor::AuditReport>, (StatusCode, Json<serde_json::Value>)> {
    let mut file_content: Option<Vec<u8>> = None;
    let mut file_name = String::from("uploaded_file");

    while let Ok(Some(field)) = multipart.next_field().await {
        if let Some(name) = field.file_name() {
            file_name = name.to_string();
        }
        let data = field.bytes().await.map_err(|err| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Failed reading file field: {}", err)
                })),
            )
        })?;
        file_content = Some(data.to_vec());
        break;
    }

    let bytes = match file_content {
        Some(b) if !b.is_empty() => b,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "No file uploaded or file was empty"
                })),
            ));
        }
    };

    let text = String::from_utf8(bytes).map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("Uploaded file '{}' is not valid UTF-8 text: {}", file_name, err)
            })),
        )
    })?;

    let invoice: auditor::InvoiceData = serde_json::from_str(&text).map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("Failed to parse JSON file '{}': {}", file_name, err),
                "hint": "Ensure the file contains valid JSON matching the invoice schema."
            })),
        )
    })?;

    tracing::info!("Successfully parsed and auditing file: {}", file_name);
    let report = auditor::audit_invoice(&invoice);
    Ok(Json(report))
}
