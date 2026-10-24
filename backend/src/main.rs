mod auditor;
mod parser;
mod pdf_scanner;

use axum::{
    body::Bytes,
    extract::Multipart,
    http::{HeaderMap, Method, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Attempts to load simple KEY=VALUE pairs from a .env file if present
fn load_dotenv_file() {
    let candidate_paths = [
        PathBuf::from(".env"),
        PathBuf::from("backend/.env"),
        PathBuf::from("../backend/.env"),
        PathBuf::from("../.env"),
    ];

    for path in candidate_paths {
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                tracing::info!("Loading environment variables from {}", path.display());
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.starts_with('#') {
                        continue;
                    }
                    if let Some((k, v)) = trimmed.split_once('=') {
                        let key = k.trim();
                        let val = v.trim().trim_matches('"').trim_matches('\'');
                        if std::env::var(key).is_err() {
                            std::env::set_var(key, val);
                        }
                    }
                }
                break;
            }
        }
    }
}

fn get_server_api_key() -> Option<String> {
    std::env::var("GEMINI_API_KEY")
        .ok()
        .or_else(|| std::env::var("GEMMA_API_KEY").ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn resolve_api_key(headers: &HeaderMap, form_api_key: Option<String>) -> Option<String> {
    headers
        .get("x-gemini-api-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or(form_api_key.filter(|s| !s.is_empty()))
        .or_else(get_server_api_key)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "invoice_auditor_backend=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    load_dotenv_file();

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
        .route("/api/extract", post(extract_handler))
        .route("/api/chat", post(chat_handler))
        .layer(cors);

    // Build final app: API routes first, then fall back to serving frontend static files
    let app = if frontend_dist.exists() {
        tracing::info!("Serving frontend from: {}", frontend_dist.display());
        let serve_dir = ServeDir::new(&frontend_dist)
            .not_found_service(ServeFile::new(frontend_dist.join("index.html")));
        api_routes.fallback_service(serve_dir)
    } else {
        tracing::warn!(
            "Frontend dist not found at '{}'. Run 'cd frontend && pnpm build' to enable serving the UI from the backend.",
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
    let server_key_set = get_server_api_key().is_some();
    Json(json!({
        "status": "ok",
        "service": "invoice-auditor",
        "version": "0.2.0",
        "has_server_key": server_key_set,
        "features": {
            "gemma_vision": true,
            "streaming_chat": true,
            "resilient_parser": true,
            "digital_pdf_reader": true
        }
    }))
}

async fn audit_json_handler(
    body_bytes: Bytes,
) -> Result<Json<auditor::AuditReport>, (StatusCode, Json<serde_json::Value>)> {
    let text = String::from_utf8(body_bytes.to_vec()).map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": format!("Payload is not valid UTF-8 text: {}", err)
            })),
        )
    })?;

    let (invoice, extraction_meta) = parser::parse_flexible_invoice_with_report(&text).map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": format!("Failed to parse invoice JSON: {}", err),
                "hint": "Check payload structure or format."
            })),
        )
    })?;

    tracing::info!("Audited JSON invoice id: {}", invoice.invoice_id);
    let report = auditor::audit_invoice_with_extraction(&invoice, Some(extraction_meta));
    Ok(Json(report))
}

async fn upload_handler(
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<auditor::AuditReport>, (StatusCode, Json<serde_json::Value>)> {
    let mut file_content: Option<Vec<u8>> = None;
    let mut file_name = String::from("uploaded_document");
    let mut form_api_key: Option<String> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let field_name = field.name().unwrap_or_default().to_string();

        if field_name == "api_key" || field_name == "apiKey" {
            if let Ok(text) = field.text().await {
                if !text.trim().is_empty() {
                    form_api_key = Some(text.trim().to_string());
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
                Json(json!({
                    "error": "No file received in multipart request or file was empty"
                })),
            ));
        }
    };

    let resolved_key = resolve_api_key(&headers, form_api_key);

    let is_pdf = pdf_scanner::is_pdf(&bytes, &file_name);
    let is_image = pdf_scanner::is_image_file(&bytes, &file_name);

    let (invoice, extraction_meta) = if is_pdf {
        tracing::info!("Processing PDF file '{}' ({} bytes)...", file_name, bytes.len());
        pdf_scanner::process_pdf(&bytes, resolved_key.as_deref())
            .await
            .map_err(|err| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": format!("Failed extracting PDF invoice '{}': {}", file_name, err),
                        "hint": "Ensure PDF contains recognizable text or set GEMINI_API_KEY for vision OCR on scanned images."
                    })),
                )
            })?
    } else if is_image {
        tracing::info!("Processing image file '{}' ({} bytes)...", file_name, bytes.len());
        let api_key = resolved_key.ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "Image and scanned photo invoices require a Gemini API key.",
                    "hint": "Set GEMINI_API_KEY in backend/.env or enter it in the browser's 'Gemma AI' settings dialog."
                })),
            )
        })?;

        let mime = pdf_scanner::detect_image_mime(&bytes, &file_name).to_string();
        let b64 = STANDARD.encode(&bytes);
        pdf_scanner::scan_images_with_gemini(&[(mime, b64)], &api_key)
            .await
            .map_err(|err| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": format!("Gemma Vision OCR failed for '{}': {}", file_name, err),
                        "hint": "Check that the image is legible and the API key has quota."
                    })),
                )
            })?
    } else {
        tracing::info!("Received text/JSON file '{}' ({} bytes). Normalizing and parsing...", file_name, bytes.len());
        let text = String::from_utf8(bytes).map_err(|err| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": format!("Uploaded file '{}' is not valid UTF-8 text: {}", file_name, err)
                })),
            )
        })?;

        parser::parse_flexible_invoice_with_report(&text).map_err(|err| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": format!("Failed to parse JSON file '{}': {}", file_name, err),
                    "hint": "Check that the file contains invoice data."
                })),
            )
        })?
    };

    tracing::info!("Successfully parsed document for invoice ID: {}", invoice.invoice_id);
    let report = auditor::audit_invoice_with_extraction(&invoice, Some(extraction_meta));
    Ok(Json(report))
}

#[derive(Debug, Deserialize)]
pub struct ExtractImagePayload {
    pub mime_type: Option<String>,
    pub data: String, // base64 string
}

#[derive(Debug, Deserialize)]
pub struct ExtractRequestPayload {
    pub text: Option<String>,
    pub images: Option<Vec<ExtractImagePayload>>, // Up to 8 images
}

async fn extract_handler(
    headers: HeaderMap,
    Json(payload): Json<ExtractRequestPayload>,
) -> Result<Json<auditor::AuditReport>, (StatusCode, Json<serde_json::Value>)> {
    let resolved_key = resolve_api_key(&headers, None);

    // If multi-page images are provided
    if let Some(imgs) = payload.images.filter(|list| !list.is_empty()) {
        let api_key = resolved_key.ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "Gemma Vision OCR requires a Gemini API key.",
                    "hint": "Configure GEMINI_API_KEY in backend/.env or enter it in the browser's 'Gemma AI' settings dialog."
                })),
            )
        })?;

        let mut image_tuples = Vec::new();
        for img in imgs.into_iter().take(8) {
            let mime = img.mime_type.unwrap_or_else(|| "image/jpeg".to_string());
            let clean_data = img.data.trim();
            // Strip data:image/...;base64, prefix if present
            let b64 = if let Some(idx) = clean_data.find(";base64,") {
                clean_data[idx + 8..].to_string()
            } else {
                clean_data.to_string()
            };
            image_tuples.push((mime, b64));
        }

        let (invoice, extraction_meta) = pdf_scanner::scan_images_with_gemini(&image_tuples, &api_key)
            .await
            .map_err(|err| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": format!("Gemma Vision extraction failed: {}", err),
                        "hint": "Check image dimensions and API key quota."
                    })),
                )
            })?;

        let report = auditor::audit_invoice_with_extraction(&invoice, Some(extraction_meta));
        return Ok(Json(report));
    }

    // If text is provided
    if let Some(text) = payload.text.filter(|t| !t.trim().is_empty()) {
        // If an API key is available, use Gemma to extract free-form text verbatim
        if let Some(ref api_key) = resolved_key {
            match pdf_scanner::scan_text_with_gemini(&text, api_key).await {
                Ok((invoice, extraction_meta)) => {
                    let report = auditor::audit_invoice_with_extraction(&invoice, Some(extraction_meta));
                    return Ok(Json(report));
                }
                Err(err) => {
                    tracing::warn!("Gemma text extraction failed: {}. Falling back to resilient parser.", err);
                }
            }
        }

        // Fallback to built-in resilient parser
        let (invoice, extraction_meta) = parser::parse_flexible_invoice_with_report(&text).map_err(|err| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": format!("Failed parsing text input: {}", err),
                    "hint": "Set a Gemini API key for Gemma free-form text parsing or provide valid JSON notation."
                })),
            )
        })?;

        let report = auditor::audit_invoice_with_extraction(&invoice, Some(extraction_meta));
        return Ok(Json(report));
    }

    Err((
        StatusCode::BAD_REQUEST,
        Json(json!({
            "error": "Payload must include either 'images' (base64) or 'text'"
        })),
    ))
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ChatMessage {
    pub role: String, // "user" | "assistant" | "model"
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub audit_report: Option<auditor::AuditReport>,
    pub invoice: Option<auditor::InvoiceData>,
}

async fn chat_handler(
    headers: HeaderMap,
    Json(payload): Json<ChatRequest>,
) -> Response {
    let resolved_key = resolve_api_key(&headers, None);

    let api_key = match resolved_key {
        Some(k) if !k.is_empty() => k,
        _ => {
            // Return informative fallback stream if no API key is configured
            let message = "To enable the streaming Gemma AI chat assistant, please configure a Gemini API key in backend/.env or open the 'Gemma AI' settings dialog in the top header. In Native mode, rule-based auditing, digital PDF text extraction, and resilient JSON normalization work fully offline without an API key!";
            let sse_data = format!("data: {}\n\ndata: [DONE]\n\n", json!({ "text": message }));
            return (
                [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
                sse_data,
            ).into_response();
        }
    };

    let client = reqwest::Client::new();
    let model = "gemini-1.5-flash";
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
        model, api_key
    );

    // Build system instructions with audit report context
    let mut audit_context = String::new();
    if let Some(ref report) = payload.audit_report {
        audit_context.push_str("\n--- ACTIVE AUDIT REPORT FINDINGS ---\n");
        audit_context.push_str(&format!("Invoice ID: {}\n", report.invoice_id));
        audit_context.push_str(&format!("Vendor Name: {}\n", report.vendor_name));
        audit_context.push_str(&format!("Overall Status: {:?}\n", report.overall_status));
        audit_context.push_str(&format!(
            "Flags Summary: Total = {}, High Risk = {}, Medium Risk = {}, Low Risk = {}\n",
            report.total_flags, report.high_risk_count, report.medium_risk_count, report.low_risk_count
        ));
        for (i, flag) in report.flags.iter().enumerate() {
            audit_context.push_str(&format!(
                "  Flag #{}: [{:?}] {} (Rule: {}) -> {}\n",
                i + 1, flag.risk_level, flag.title, flag.rule_id, flag.description
            ));
        }
        if let Some(ref ext) = report.extraction {
            audit_context.push_str(&format!("Extraction Method: {} (Model: {}, Confidence: {})\n", ext.method, ext.model, ext.confidence));
            if !ext.derived_fields.is_empty() {
                audit_context.push_str(&format!("Derived / Defaulted Fields: {:?}\n", ext.derived_fields));
            }
        }
        audit_context.push_str(&format!(
            "Parsed Invoice: Date={}, Currency={}, Subtotal={:.2}, Tax={:.2}, Total Amount={:.2}, Line Items Count={}\n",
            report.parsed_invoice.invoice_date, report.parsed_invoice.currency,
            report.parsed_invoice.subtotal, report.parsed_invoice.tax,
            report.parsed_invoice.total_amount, report.parsed_invoice.items.len()
        ));
        for (i, item) in report.parsed_invoice.items.iter().enumerate() {
            audit_context.push_str(&format!(
                "    Item #{}: '{}' (qty: {:.2}, unit_price: {:.2}, total: {:.2})\n",
                i + 1, item.description, item.quantity, item.unit_price, item.total
            ));
        }
        audit_context.push_str("--- END AUDIT FINDINGS ---\n");
    }

    let system_prompt = format!(
        "You are DocuAudit AI, an expert invoice auditor assistant powered by Gemma. You are helping an auditor review the document and report findings.{}
Always reference specific figures, line items, discrepancy math, and corporate audit rules.
Be concise, helpful, and professional. Provide exact calculation breakdowns or draft vendor query emails when requested.",
        audit_context
    );

    // Format Gemini contents
    let mut contents: Vec<serde_json::Value> = Vec::new();
    for msg in payload.messages {
        let role = if msg.role == "assistant" || msg.role == "model" {
            "model"
        } else {
            "user"
        };
        contents.push(json!({
            "role": role,
            "parts": [{ "text": msg.content }]
        }));
    }

    let gemini_body = json!({
        "system_instruction": {
            "parts": [{ "text": system_prompt }]
        },
        "contents": contents
    });

    let res = match client.post(&url).json(&gemini_body).send().await {
        Ok(r) => r,
        Err(err) => {
            let error_json = json!({ "error": format!("Failed to initiate chat stream: {}", err) });
            let sse_data = format!("data: {}\n\ndata: [DONE]\n\n", error_json);
            return (
                [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
                sse_data,
            ).into_response();
        }
    };

    if !res.status().is_success() {
        let err_text = res.text().await.unwrap_or_default();
        let error_json = json!({ "error": format!("Gemini API error: {}", err_text) });
        let sse_data = format!("data: {}\n\ndata: [DONE]\n\n", error_json);
        return (
            [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
            sse_data,
        ).into_response();
    }

    // Forward byte stream directly as SSE
    let byte_stream = res.bytes_stream().map(|chunk_result| {
        match chunk_result {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                let mut out = String::new();
                for line in text.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("data: ") {
                        let json_str = &trimmed[6..];
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) {
                            if let Some(candidate_text) = v["candidates"][0]["content"]["parts"][0]["text"].as_str() {
                                out.push_str(&format!("data: {}\n\n", json!({ "text": candidate_text })));
                            }
                        }
                    }
                }
                if out.is_empty() {
                    Ok::<_, Infallible>(bytes)
                } else {
                    Ok::<_, Infallible>(Bytes::from(out))
                }
            }
            Err(_) => Ok::<_, Infallible>(Bytes::new()),
        }
    });

    Response::builder()
        .header(axum::http::header::CONTENT_TYPE, "text/event-stream")
        .header(axum::http::header::CACHE_CONTROL, "no-cache")
        .header(axum::http::header::CONNECTION, "keep-alive")
        .body(axum::body::Body::from_stream(byte_stream))
        .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "SSE response failed").into_response())
}
