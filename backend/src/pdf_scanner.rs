use crate::auditor::{ExtractionMetadata, InvoiceData, LineItem};
use crate::parser::parse_flexible_invoice_with_report;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use reqwest::Client;
use serde_json::json;
use std::time::Duration;

/// Checks if bytes or filename indicate a PDF document
pub fn is_pdf(bytes: &[u8], filename: &str) -> bool {
    let lower_name = filename.to_lowercase();
    if lower_name.ends_with(".pdf") {
        return true;
    }
    // PDF Magic bytes: %PDF-
    if bytes.len() >= 5 && &bytes[0..5] == b"%PDF-" {
        return true;
    }
    false
}

/// Checks if bytes or filename indicate an image file
pub fn is_image_file(bytes: &[u8], filename: &str) -> bool {
    let lower = filename.to_lowercase();
    if lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".webp")
    {
        return true;
    }
    // Magic bytes: PNG, JPEG, WEBP
    if bytes.len() >= 8 {
        if &bytes[0..8] == b"\x89PNG\r\n\x1a\n" {
            return true;
        }
        if &bytes[0..3] == b"\xff\xd8\xff" {
            return true;
        }
        if &bytes[0..4] == b"RIFF" && bytes.len() >= 12 && &bytes[8..12] == b"WEBP" {
            return true;
        }
    }
    false
}

/// Detects MIME type for image bytes
pub fn detect_image_mime(bytes: &[u8], filename: &str) -> &'static str {
    let lower = filename.to_lowercase();
    if lower.ends_with(".png") || (bytes.len() >= 8 && &bytes[0..8] == b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if lower.ends_with(".webp") || (bytes.len() >= 12 && &bytes[8..12] == b"WEBP") {
        "image/webp"
    } else {
        "image/jpeg"
    }
}

/// Extracts text from PDF bytes natively with stream fallback for non-standard PDFs
pub fn extract_text_native(pdf_bytes: &[u8]) -> Result<String, String> {
    if let Ok(text) = pdf_extract::extract_text_from_mem(pdf_bytes) {
        if !text.trim().is_empty() {
            return Ok(text);
        }
    }

    // Stream fallback: extracts (string) Tj literals from raw stream
    let fallback = extract_text_fallback_stream(pdf_bytes);
    if !fallback.trim().is_empty() {
        return Ok(fallback);
    }

    Err("The PDF appears to be a scanned image with no embedded text layer. Please configure a Gemini API key to enable Gemma Vision OCR for scanned documents.".to_string())
}

/// Fallback text stream parser for PDFs with raw streams or missing ToUnicode CMaps
pub fn extract_text_fallback_stream(bytes: &[u8]) -> String {
    let mut extracted = String::new();
    let text = String::from_utf8_lossy(bytes);

    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '(' {
            let start = i + 1;
            let mut end = start;
            let mut escaped = false;
            while end < chars.len() {
                if escaped {
                    escaped = false;
                } else if chars[end] == '\\' {
                    escaped = true;
                } else if chars[end] == ')' {
                    break;
                }
                end += 1;
            }
            if end < chars.len() && chars[end] == ')' {
                let piece: String = chars[start..end].iter().collect();
                if piece.len() >= 2 && piece.chars().any(|c| c.is_alphanumeric()) {
                    extracted.push_str(&piece);
                    extracted.push('\n');
                }
                i = end;
            }
        }
        i += 1;
    }

    extracted
}

/// Heuristically converts raw extracted PDF text into an InvoiceData structure
pub fn parse_pdf_text_heuristically(text: &str) -> Result<(InvoiceData, ExtractionMetadata), String> {
    let lines: Vec<&str> = text.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    if lines.is_empty() {
        return Err("The PDF text layer is empty. Configure a Gemini API key for Gemma Vision OCR on scanned documents.".to_string());
    }

    let mut derived_fields = Vec::new();
    let mut vendor_name = String::new();
    let mut invoice_id = String::new();
    let mut invoice_date = String::new();
    let mut subtotal: Option<f64> = None;
    let mut tax: Option<f64> = None;
    let mut total_amount: Option<f64> = None;
    let items: Vec<LineItem> = Vec::new();

    // Vendor is usually in the first few lines
    for line in lines.iter().take(4) {
        let lower = line.to_lowercase();
        if !lower.contains("invoice") && !lower.contains("bill to") && line.len() > 2 {
            vendor_name = line.to_string();
            break;
        }
    }

    let currency = if text.contains('₹') || text.to_uppercase().contains("INR") || text.contains("Rs") {
        derived_fields.push("Detected currency INR from ₹ / Rs symbol in text layer".to_string());
        "INR".to_string()
    } else {
        derived_fields.push("Defaulted currency to USD".to_string());
        "USD".to_string()
    };

    // Scan lines for fields
    for line in &lines {
        let lower = line.to_lowercase();

        // Invoice ID
        if invoice_id.is_empty() {
            if let Some(idx) = lower.find("invoice #").or_else(|| lower.find("invoice no")).or_else(|| lower.find("inv #")) {
                let candidate = line[idx..].split(|c: char| c == ':' || c == '#' || c.is_whitespace()).collect::<Vec<_>>();
                if let Some(val) = candidate.iter().rev().find(|s| !s.is_empty() && s.len() > 2) {
                    invoice_id = val.to_string();
                }
            }
        }

        // Date
        if invoice_date.is_empty() {
            if lower.contains("date:") || lower.contains("date :") || lower.contains("invoice date") {
                if let Some(idx) = line.find(':') {
                    let d = line[idx + 1..].trim();
                    if !d.is_empty() {
                        invoice_date = d.to_string();
                    }
                }
            }
        }

        // Subtotal
        if subtotal.is_none() && (lower.contains("subtotal") || lower.contains("sub total") || lower.contains("net amount")) {
            subtotal = extract_trailing_number(line);
        }

        // Tax
        if tax.is_none() && (lower.contains("tax") || lower.contains("vat") || lower.contains("gst")) && !lower.contains("taxable") {
            tax = extract_trailing_number(line);
        }

        // Total
        if lower.contains("total amount") || lower.contains("grand total") || lower.starts_with("total") || lower.contains("balance due") {
            if let Some(amt) = extract_trailing_number(line) {
                total_amount = Some(amt);
            }
        }
    }

    if invoice_id.is_empty() {
        invoice_id = format!("INV-PDF-{}", &chrono::Utc::now().timestamp_millis().to_string()[7..]);
        derived_fields.push(format!("Missing invoice ID in PDF text layer; generated fallback '{}'", invoice_id));
    }
    if vendor_name.is_empty() {
        vendor_name = "Extracted PDF Vendor".to_string();
        derived_fields.push("Vendor name missing in PDF text layer; defaulted to 'Extracted PDF Vendor'".to_string());
    }

    let sub = subtotal.unwrap_or(0.0);
    let tx = tax.unwrap_or(0.0);
    let tot = total_amount.unwrap_or_else(|| {
        let t = if sub > 0.0 { sub + tx } else { 0.0 };
        derived_fields.push(format!("Missing total amount in PDF text layer; derived from subtotal ({:.2}) + tax ({:.2}) = {:.2}", sub, tx, t));
        t
    });

    if items.is_empty() {
        derived_fields.push("No itemized line item rows could be segmented from plain text layer".to_string());
    }

    let metadata = ExtractionMetadata {
        method: "Digital PDF Text Layer".to_string(),
        model: "pdf-extract (native)".to_string(),
        confidence: "High".to_string(),
        notes: "Read directly from digital PDF embedded text layer without requiring AI.".to_string(),
        derived_fields,
    };

    let invoice = InvoiceData {
        invoice_id,
        vendor_name,
        invoice_date: if invoice_date.is_empty() {
            chrono::Utc::now().format("%Y-%m-%d").to_string()
        } else {
            invoice_date
        },
        currency,
        items,
        subtotal: sub,
        tax: tx,
        total_amount: tot,
    };

    Ok((invoice, metadata))
}

fn extract_trailing_number(line: &str) -> Option<f64> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    for part in parts.iter().rev() {
        let cleaned: String = part.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
        if let Ok(num) = cleaned.parse::<f64>() {
            return Some(num);
        }
    }
    None
}

/// Prompt enforcing the critical verbatim transcription requirement:
/// Gemma ONLY transcribes and NEVER recomputes amounts, so wrong totals are still caught!
const GEMMA_VERBATIM_INVOICE_PROMPT: &str = r#"You are an automated invoice transcription system powered by Gemma.
CRITICAL AUDIT RULE:
1. You MUST ONLY transcribe and extract the text and numbers EXACTLY as printed on the document.
2. DO NOT recalculate, adjust, correct, or recompute any numbers (totals, subtotals, tax, quantities, unit prices).
3. If numbers do not add up, or if calculations on the invoice are mathematically incorrect, transcribe the exact incorrect numbers as printed. Preserving discrepancies is required for corporate audit compliance!
4. If multiple pages/images are provided, treat them as consecutive pages of one single invoice.

Output ONLY a valid JSON object matching this schema:
{
  "invoice_id": "string",
  "vendor_name": "string",
  "invoice_date": "YYYY-MM-DD",
  "currency": "USD or INR or currency code/symbol",
  "items": [
    {
      "description": "string",
      "quantity": 1.0,
      "unit_price": 10.0,
      "total": 10.0
    }
  ],
  "subtotal": 0.0,
  "tax": 0.0,
  "total_amount": 0.0
}
Return only valid JSON notation without commentary."#;

/// Extracts invoice from PDF bytes using Gemini / Gemma multimodal API
pub async fn scan_pdf_with_gemini(
    pdf_bytes: &[u8],
    api_key: &str,
) -> Result<(InvoiceData, ExtractionMetadata), String> {
    let base64_pdf = STANDARD.encode(pdf_bytes);
    let client = Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let model = "gemini-1.5-flash";
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, api_key
    );

    let payload = json!({
        "contents": [{
            "parts": [
                {
                    "inline_data": {
                        "mime_type": "application/pdf",
                        "data": base64_pdf
                    }
                },
                {
                    "text": GEMMA_VERBATIM_INVOICE_PROMPT
                }
            ]
        }],
        "generationConfig": {
            "responseMimeType": "application/json"
        }
    });

    let res = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Network request to Gemma API failed: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        return Err(format!("Gemma API error ({}): {}", status, err_text));
    }

    let res_json: serde_json::Value = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse Gemma API JSON response: {}", e))?;

    let text = res_json["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .ok_or_else(|| "Gemma API returned empty response text".to_string())?;

    let (invoice, mut meta) = parse_flexible_invoice_with_report(text)?;
    meta.method = "Gemma Vision OCR".to_string();
    meta.model = model.to_string();
    meta.confidence = "High".to_string();
    meta.notes = "Gemma transcribed scanned PDF verbatim preserving arithmetic discrepancies.".to_string();

    Ok((invoice, meta))
}

/// Extracts invoice from multiple image pages (PNG, JPG, WEBP) using Gemma Vision API
pub async fn scan_images_with_gemini(
    images: &[ (String, String) ], // Vec of (mime_type, base64_data), up to 8 images
    api_key: &str,
) -> Result<(InvoiceData, ExtractionMetadata), String> {
    if images.is_empty() {
        return Err("No image data provided for extraction".to_string());
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let model = "gemini-1.5-flash";
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, api_key
    );

    let mut parts: Vec<serde_json::Value> = Vec::new();
    for (_idx, (mime, data)) in images.iter().take(8).enumerate() {
        parts.push(json!({
            "inline_data": {
                "mime_type": mime,
                "data": data
            }
        }));
    }

    parts.push(json!({
        "text": format!("{} Note: You are viewing {} page(s) of a single invoice.", GEMMA_VERBATIM_INVOICE_PROMPT, images.len())
    }));

    let payload = json!({
        "contents": [{
            "parts": parts
        }],
        "generationConfig": {
            "responseMimeType": "application/json"
        }
    });

    let res = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Network request to Gemma Vision API failed: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        return Err(format!("Gemma Vision API error ({}): {}", status, err_text));
    }

    let res_json: serde_json::Value = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse Gemma Vision API JSON response: {}", e))?;

    let text = res_json["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .ok_or_else(|| "Gemma Vision API returned empty response text".to_string())?;

    let (invoice, mut meta) = parse_flexible_invoice_with_report(text)?;
    meta.method = "Gemma Vision OCR".to_string();
    meta.model = model.to_string();
    meta.confidence = "High".to_string();
    meta.notes = format!(
        "Transcribed {} preprocessed page image(s) verbatim without recomputing arithmetic amounts.",
        images.len()
    );

    Ok((invoice, meta))
}

/// Extracts structured invoice from free-form or messy unstructured text using Gemma
pub async fn scan_text_with_gemini(
    raw_text: &str,
    api_key: &str,
) -> Result<(InvoiceData, ExtractionMetadata), String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let model = "gemini-1.5-flash";
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, api_key
    );

    let prompt = format!(
        "{}\n\nDOCUMENT TEXT TO TRANSCRIBE:\n{}",
        GEMMA_VERBATIM_INVOICE_PROMPT, raw_text
    );

    let payload = json!({
        "contents": [{
            "parts": [{ "text": prompt }]
        }],
        "generationConfig": {
            "responseMimeType": "application/json"
        }
    });

    let res = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Gemma Text API request failed: {}", e))?;

    if !res.status().is_success() {
        let err = res.text().await.unwrap_or_default();
        return Err(format!("Gemma Text API error: {}", err));
    }

    let json_res: serde_json::Value = res
        .json()
        .await
        .map_err(|e| format!("Failed parsing Gemma response: {}", e))?;

    let output_text = json_res["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .ok_or_else(|| "Empty response from Gemma Text API".to_string())?;

    let (invoice, mut meta) = parse_flexible_invoice_with_report(output_text)?;
    meta.method = "Gemma Free-form Text".to_string();
    meta.model = model.to_string();
    meta.confidence = "High".to_string();
    meta.notes = "Mapped unstructured free-form text into structured invoice verbatim.".to_string();

    Ok((invoice, meta))
}

/// Primary PDF Processor:
/// - First tries native text layer extraction (Digital PDF).
/// - If scanned or text layer is missing, uses Gemma Vision when API key is provided.
pub async fn process_pdf(
    pdf_bytes: &[u8],
    api_key_override: Option<&str>,
) -> Result<(InvoiceData, ExtractionMetadata), String> {
    let resolved_api_key = api_key_override
        .map(|s| s.to_string())
        .or_else(|| std::env::var("GEMINI_API_KEY").ok())
        .or_else(|| std::env::var("GEMMA_API_KEY").ok());

    // 1. First try native PDF text layer extraction (Digital PDF - works without any AI key!)
    match extract_text_native(pdf_bytes) {
        Ok(native_text) if native_text.trim().len() > 20 => {
            tracing::info!("Successfully read text layer from digital PDF ({} chars).", native_text.len());
            // If the digital PDF text contains JSON notation, attempt resilient JSON parse first
            if native_text.contains('{') && native_text.contains('}') {
                if let Ok((inv, mut meta)) = parse_flexible_invoice_with_report(&native_text) {
                    meta.method = "Digital PDF (Embedded JSON)".to_string();
                    meta.notes = "Extracted structured invoice data directly from PDF embedded text layer without AI.".to_string();
                    return Ok((inv, meta));
                }
            }
            // Heuristic text parsing of digital PDF
            return parse_pdf_text_heuristically(&native_text);
        }
        _ => {
            tracing::info!("PDF has no embedded text layer or is a scanned image.");
        }
    }

    // 2. If no text layer found (scanned PDF), use Gemma Vision OCR if an API key is available
    if let Some(key) = resolved_api_key {
        if !key.trim().is_empty() {
            tracing::info!("Scanning PDF with Google Gemma multimodal API...");
            return scan_pdf_with_gemini(pdf_bytes, key.trim()).await;
        }
    }

    Err(
        "This PDF is a scanned image with no embedded text layer. Please configure a Gemini API key in backend/.env or in the browser's 'Gemma AI' settings dialog to enable Gemma Vision OCR for scanned PDFs.".to_string()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_from_sample_pdf() {
        let bytes = std::fs::read("../samples/sample_invoice.pdf").unwrap();
        let text = extract_text_native(&bytes).unwrap();
        assert!(text.contains("Quantum Compute Grid Ltd."));
        assert!(text.contains("INV-PDF-2026-900"));
        assert!(text.contains("3850.00"));
    }

    #[test]
    fn test_parse_sample_pdf_text() {
        let sample_text = "Quantum Compute Grid Ltd.\nINVOICE\nInvoice #: INV-PDF-2026-900\nDate: 2026-10-24\nSubtotal: 3500.00\nTax: 350.00\nTotal: 3850.00\n";
        let (invoice, meta) = parse_pdf_text_heuristically(sample_text).expect("Should parse sample PDF text");
        assert_eq!(invoice.invoice_id, "INV-PDF-2026-900");
        assert_eq!(invoice.vendor_name, "Quantum Compute Grid Ltd.");
        assert_eq!(invoice.subtotal, 3500.0);
        assert_eq!(invoice.tax, 350.0);
        assert_eq!(invoice.total_amount, 3850.0);
        assert_eq!(meta.method, "Digital PDF Text Layer");
    }
}
