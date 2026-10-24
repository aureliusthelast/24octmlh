mod auditor;

use axum::{
    extract::Multipart,
    http::{Method, StatusCode},
    response::Json,
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};
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

    let app = Router::new()
        .route("/health", get(health_check))
        .route("/api/audit-json", post(audit_json_handler))
        .route("/api/upload", post(upload_handler))
        .layer(cors);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Starting Invoice Auditor Backend on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind TCP listener to address");

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
