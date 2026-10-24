use crate::auditor::InvoiceData;
use crate::parser::parse_flexible_invoice;
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

    Err("The PDF appears to be a scanned image with no embedded text layer. Please configure GEMINI_API_KEY or an Ollama Gemma endpoint to enable vision OCR for scanned documents.".to_string())
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
pub fn parse_pdf_text_heuristically(text: &str) -> Result<InvoiceData, String> {
    let lines: Vec<&str> = text.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    if lines.is_empty() {
        return Err("The PDF appears to be a scanned image with no embedded text layer. Please configure GEMINI_API_KEY or an Ollama Gemma endpoint to enable vision OCR for scanned documents.".to_string());
    }

    let mut vendor_name = String::new();
    let mut invoice_id = String::new();
    let mut invoice_date = String::new();
    let mut currency = "USD".to_string();
    let mut subtotal: Option<f64> = None;
    let mut tax: Option<f64> = None;
    let mut total_amount: Option<f64> = None;

    // Vendor is usually in the first few lines
    for line in lines.iter().take(4) {
        let lower = line.to_lowercase();
        if !lower.contains("invoice") && !lower.contains("bill to") && line.len() > 2 {
            vendor_name = line.to_string();
            break;
        }
    }

    if text.contains('₹') || text.to_uppercase().contains("INR") || text.contains("Rs") {
        currency = "INR".to_string();
    }

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
        invoice_id = "INV-PDF-SCAN".to_string();
    }
    if vendor_name.is_empty() {
        vendor_name = "Extracted PDF Vendor".to_string();
    }

    let sub = subtotal.unwrap_or(0.0);
    let tx = tax.unwrap_or(0.0);
    let tot = total_amount.unwrap_or(if sub > 0.0 { sub + tx } else { 0.0 });

    Ok(InvoiceData {
        invoice_id,
        vendor_name,
        invoice_date: if invoice_date.is_empty() { chrono::Utc::now().format("%Y-%m-%d").to_string() } else { invoice_date },
        currency,
        items: Vec::new(),
        subtotal: sub,
        tax: tx,
        total_amount: tot,
    })
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

/// Calls Google's multimodal Gemini / Gemma API with inline PDF bytes
pub async fn scan_pdf_with_gemini(
    pdf_bytes: &[u8],
    api_key: &str,
) -> Result<InvoiceData, String> {
    let base64_pdf = STANDARD.encode(pdf_bytes);
    let client = Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    // Use fast multimodal model
    let model = "gemini-2.5-flash";
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, api_key
    );

    let prompt = r#"You are an automated invoice parsing system. Carefully examine this messy or scanned document.
Transcribe all text, totals, tax, line items, vendor details, and mathematical values.
Convert and output STRICT JSON matching this schema:
{
  "invoice_id": "string",
  "vendor_name": "string",
  "invoice_date": "YYYY-MM-DD",
  "currency": "USD or INR or currency symbol/code",
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
Return only valid JSON."#;

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
                    "text": prompt
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
        .map_err(|e| format!("Network request to Gemini/Gemma API failed: {}", e))?;

    if !res.status().is_success() {
        let err_text = res.text().await.unwrap_or_default();
        return Err(format!("Gemini/Gemma API error: {}", err_text));
    }

    let res_json: serde_json::Value = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse Gemini API JSON response: {}", e))?;

    let text = res_json["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .ok_or_else(|| "Gemini/Gemma API returned empty response text".to_string())?;

    parse_flexible_invoice(text)
}

/// Calls local Ollama Gemma instance if running
pub async fn scan_pdf_with_ollama(
    text_content: &str,
    ollama_host: &str,
) -> Result<InvoiceData, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let url = format!("{}/api/generate", ollama_host.trim_end_matches('/'));
    let prompt = format!(
        "Extract invoice information from this document text into JSON matching fields: invoice_id, vendor_name, invoice_date, currency, items (description, quantity, unit_price, total), subtotal, tax, total_amount:\n\n{}",
        text_content
    );

    let payload = json!({
        "model": "gemma2",
        "prompt": prompt,
        "format": "json",
        "stream": false
    });

    let res = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Ollama request failed: {}", e))?;

    if !res.status().is_success() {
        let err = res.text().await.unwrap_or_default();
        return Err(format!("Ollama error: {}", err));
    }

    let json_res: serde_json::Value = res
        .json()
        .await
        .map_err(|e| format!("Failed parsing Ollama response: {}", e))?;

    let output_text = json_res["response"]
        .as_str()
        .ok_or_else(|| "Empty response from Ollama Gemma".to_string())?;

    parse_flexible_invoice(output_text)
}

/// Primary PDF Processor: Uses Gemma/Gemini when configured, local Ollama, or native offline fallback
pub async fn process_pdf(
    pdf_bytes: &[u8],
    api_key_override: Option<&str>,
    ollama_host_override: Option<&str>,
) -> Result<InvoiceData, String> {
    // 1. Try Gemini / Gemma Cloud API if key is available
    let resolved_api_key = api_key_override
        .map(|s| s.to_string())
        .or_else(|| std::env::var("GEMINI_API_KEY").ok())
        .or_else(|| std::env::var("GEMMA_API_KEY").ok());

    if let Some(key) = resolved_api_key {
        if !key.trim().is_empty() {
            tracing::info!("Scanning PDF with Google Gemma / Gemini multimodal API...");
            match scan_pdf_with_gemini(pdf_bytes, key.trim()).await {
                Ok(invoice) => return Ok(invoice),
                Err(err) => {
                    tracing::warn!("Gemma API extraction failed: {}. Falling back to native/local scanner.", err);
                }
            }
        }
    }

    // 2. Try native PDF text extraction
    let native_text = match extract_text_native(pdf_bytes) {
        Ok(t) => t,
        Err(e) => {
            return Err(format!("Could not extract text from PDF: {}. (Tip: If this is a scanned photo, set GEMINI_API_KEY to use Vision OCR)", e));
        }
    };

    // 3. Try Ollama if configured
    let resolved_ollama = ollama_host_override
        .map(|s| s.to_string())
        .or_else(|| std::env::var("OLLAMA_HOST").ok());

    if let Some(host) = resolved_ollama {
        if !host.trim().is_empty() {
            tracing::info!("Querying local Ollama Gemma instance at {}...", host);
            if let Ok(invoice) = scan_pdf_with_ollama(&native_text, &host).await {
                return Ok(invoice);
            }
        }
    }

    // 4. Native heuristic extraction fallback
    tracing::info!("Extracting invoice fields from PDF using Native Heuristic Parser...");
    parse_pdf_text_heuristically(&native_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_from_sample_pdf() {
        let bytes = std::fs::read("../samples/sample_invoice.pdf").unwrap();
        let text = extract_text_native(&bytes).unwrap();
        println!("EXTRACTED TEXT:\n{}", text);
        assert!(text.contains("Quantum Compute Grid Ltd."));
        assert!(text.contains("INV-PDF-2026-900"));
        assert!(text.contains("3850.00"));
    }

    #[test]
    fn test_parse_sample_pdf_text() {
        let sample_text = "Quantum Compute Grid Ltd.\nINVOICE\nInvoice #: INV-PDF-2026-900\nDate: 2026-10-24\nSubtotal: 3500.00\nTax: 350.00\nTotal: 3850.00\n";
        let invoice = parse_pdf_text_heuristically(sample_text).expect("Should parse sample PDF text");
        assert_eq!(invoice.invoice_id, "INV-PDF-2026-900");
        assert_eq!(invoice.vendor_name, "Quantum Compute Grid Ltd.");
        assert_eq!(invoice.subtotal, 3500.0);
        assert_eq!(invoice.tax, 350.0);
        assert_eq!(invoice.total_amount, 3850.0);
    }
}
