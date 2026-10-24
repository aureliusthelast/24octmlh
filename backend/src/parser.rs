use crate::auditor::{default_currency, normalize_currency, InvoiceData, LineItem};
use chrono::Utc;
use serde_json::Value;

/// Cleans raw input by stripping BOM, trimming, and extracting JSON from markdown code blocks
pub fn clean_json_text(raw: &str) -> String {
    let mut text = raw.trim();

    // Strip UTF-8 BOM if present
    if text.starts_with('\u{FEFF}') {
        text = &text['\u{FEFF}'.len_utf8()..];
    }

    let text = text.trim();

    // If enclosed in markdown code fences, e.g. ```json ... ``` or ``` ... ```
    if let Some(start_idx) = text.find("```") {
        let content_after = &text[start_idx + 3..];
        // Skip language tag like "json" or "JSON"
        let json_start = if let Some(newline_idx) = content_after.find('\n') {
            &content_after[newline_idx + 1..]
        } else {
            content_after
        };

        if let Some(end_idx) = json_start.rfind("```") {
            return json_start[..end_idx].trim().to_string();
        }
    }

    // If there is conversational text before the first '{' or '[' and after the last '}' or ']'
    if let (Some(first_brace), Some(last_brace)) = (text.find('{'), text.rfind('}')) {
        if first_brace < last_brace {
            return text[first_brace..=last_brace].trim().to_string();
        }
    }

    text.to_string()
}

/// Robust number parser that handles numbers, string-encoded floats, currency symbols ($ , ₹ , €),
/// commas ("1,200.50"), spaces, and accounting parentheses ("(150.00)")
pub fn parse_flexible_f64(val: Option<&Value>) -> Option<f64> {
    let val = val?;
    match val {
        Value::Number(num) => num.as_f64(),
        Value::String(s) => {
            let mut cleaned = s.trim().to_string();
            if cleaned.is_empty() {
                return None;
            }

            // Check accounting format: "(100.50)" -> "-100.50"
            let is_negative = (cleaned.starts_with('(') && cleaned.ends_with(')'))
                || cleaned.starts_with('-');

            // Remove non-numeric characters except '.'
            cleaned = cleaned
                .chars()
                .filter(|c| c.is_ascii_digit() || *c == '.')
                .collect();

            if cleaned.is_empty() {
                return None;
            }

            cleaned.parse::<f64>().ok().map(|v| if is_negative { -v } else { v })
        }
        _ => None,
    }
}

/// Recursively unwraps root containers: `{ "invoice": { ... } }`, `{ "data": { ... } }`, etc.
fn unwrap_root_value(mut val: Value) -> Value {
    for _ in 0..3 {
        match val {
            Value::Object(ref map) => {
                let candidate_keys = ["invoice", "data", "document", "result", "payload", "bill"];
                let mut found_inner = None;
                for key in candidate_keys {
                    if let Some(inner) = map.get(key) {
                        if inner.is_object() {
                            found_inner = Some(inner.clone());
                            break;
                        }
                    }
                }
                if let Some(inner) = found_inner {
                    val = inner;
                    continue;
                }
                break;
            }
            Value::Array(ref arr) if arr.len() == 1 => {
                val = arr[0].clone();
            }
            _ => break,
        }
    }
    val
}

/// Flexible field lookup checking multiple common aliases and case variations
fn get_field_val<'a>(obj: &'a serde_json::Map<String, Value>, aliases: &[&str]) -> Option<&'a Value> {
    for alias in aliases {
        // Direct match
        if let Some(v) = obj.get(*alias) {
            return Some(v);
        }
        // Case-insensitive match
        for (k, v) in obj.iter() {
            if k.eq_ignore_ascii_case(alias) {
                return Some(v);
            }
        }
    }
    None
}

/// Flexible string field lookup
fn get_field_string(obj: &serde_json::Map<String, Value>, aliases: &[&str]) -> Option<String> {
    let val = get_field_val(obj, aliases)?;
    match val {
        Value::String(s) => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Detects currency symbol from raw strings if currency field is missing or generic
fn detect_currency_from_text(text: &str) -> Option<String> {
    if text.contains('₹') || text.to_uppercase().contains("INR") || text.contains("Rs") {
        Some("INR".to_string())
    } else if text.contains('$') || text.to_uppercase().contains("USD") {
        Some("USD".to_string())
    } else if text.contains('€') || text.to_uppercase().contains("EUR") {
        Some("EUR".to_string())
    } else {
        None
    }
}

/// Parses a flexible LineItem
fn parse_line_item(item_val: &Value) -> Option<LineItem> {
    match item_val {
        Value::Object(map) => {
            let description = get_field_string(
                map,
                &[
                    "description",
                    "desc",
                    "item",
                    "name",
                    "title",
                    "product_name",
                    "product",
                    "service",
                    "particulars",
                ],
            )
            .unwrap_or_else(|| "Line Item".to_string());

            let quantity = parse_flexible_f64(get_field_val(
                map,
                &["quantity", "qty", "count", "units", "number", "hours"],
            ))
            .unwrap_or(1.0);

            let unit_price = parse_flexible_f64(get_field_val(
                map,
                &[
                    "unit_price",
                    "unitPrice",
                    "price",
                    "rate",
                    "cost",
                    "unit_cost",
                    "unitCost",
                ],
            ));

            let total = parse_flexible_f64(get_field_val(
                map,
                &[
                    "total",
                    "amount",
                    "line_total",
                    "lineTotal",
                    "item_total",
                    "itemTotal",
                    "price_total",
                ],
            ));

            let (final_unit_price, final_total) = match (unit_price, total) {
                (Some(up), Some(t)) => (up, t),
                (Some(up), None) => (up, up * quantity),
                (None, Some(t)) => (if quantity > 0.0 { t / quantity } else { t }, t),
                (None, None) => (0.0, 0.0),
            };

            Some(LineItem {
                description,
                quantity,
                unit_price: final_unit_price,
                total: final_total,
            })
        }
        _ => None,
    }
}

/// Master function: parses any invoice JSON text with extreme resilience
pub fn parse_flexible_invoice(raw_text: &str) -> Result<InvoiceData, String> {
    let cleaned = clean_json_text(raw_text);
    if cleaned.is_empty() {
        return Err("Input invoice payload is empty".to_string());
    }

    let parsed_val: Value = serde_json::from_str(&cleaned).map_err(|e| {
        format!(
            "Invalid JSON format: {}. Ensure payload contains valid JSON notation.",
            e
        )
    })?;

    let root = unwrap_root_value(parsed_val);
    let map = match root {
        Value::Object(m) => m,
        _ => {
            return Err("Expected JSON object at root of invoice payload".to_string());
        }
    };

    // 1. Invoice ID
    let invoice_id = get_field_string(
        &map,
        &[
            "invoice_id",
            "invoiceId",
            "id",
            "invoice_number",
            "invoiceNumber",
            "inv_no",
            "invNo",
            "bill_no",
            "billNumber",
            "reference",
            "reference_number",
        ],
    )
    .unwrap_or_else(|| {
        format!("INV-{}", &Utc::now().timestamp_millis().to_string()[7..])
    });

    // 2. Vendor Name
    let vendor_name = get_field_string(
        &map,
        &[
            "vendor_name",
            "vendorName",
            "vendor",
            "supplier",
            "supplier_name",
            "supplierName",
            "seller",
            "seller_name",
            "merchant",
            "biller",
            "company",
            "from",
        ],
    )
    .unwrap_or_default();

    // 3. Invoice Date
    let invoice_date = get_field_string(
        &map,
        &[
            "invoice_date",
            "invoiceDate",
            "date",
            "bill_date",
            "issue_date",
            "issueDate",
            "created_at",
            "timestamp",
        ],
    )
    .unwrap_or_else(|| Utc::now().format("%Y-%m-%d").to_string());

    // 4. Currency
    let raw_currency = get_field_string(
        &map,
        &[
            "currency",
            "currency_code",
            "currencyCode",
            "curr",
            "symbol",
        ],
    )
    .or_else(|| detect_currency_from_text(&cleaned))
    .unwrap_or_else(default_currency);

    let currency = normalize_currency(&raw_currency);

    // 5. Line items
    let mut items = Vec::new();
    if let Some(items_val) = get_field_val(
        &map,
        &[
            "items",
            "line_items",
            "lineItems",
            "products",
            "entries",
            "details",
            "records",
        ],
    ) {
        if let Value::Array(arr) = items_val {
            for item in arr {
                if let Some(parsed_item) = parse_line_item(item) {
                    items.push(parsed_item);
                }
            }
        }
    }

    // 6. Subtotal
    let subtotal_opt = parse_flexible_f64(get_field_val(
        &map,
        &[
            "subtotal",
            "sub_total",
            "subTotal",
            "net_amount",
            "netAmount",
            "taxable_amount",
            "taxableAmount",
            "amount_before_tax",
        ],
    ));

    // Calculate sum of line items if available
    let line_items_sum: f64 = items.iter().map(|it| it.total).sum();

    let subtotal = subtotal_opt.unwrap_or_else(|| {
        if !items.is_empty() {
            line_items_sum
        } else {
            0.0
        }
    });

    // 7. Tax
    let tax = parse_flexible_f64(get_field_val(
        &map,
        &[
            "tax",
            "tax_amount",
            "taxAmount",
            "tax_total",
            "vat",
            "gst",
            "sales_tax",
            "total_tax",
        ],
    ))
    .unwrap_or(0.0);

    // 8. Total Amount
    let total_opt = parse_flexible_f64(get_field_val(
        &map,
        &[
            "total_amount",
            "totalAmount",
            "total",
            "grand_total",
            "grandTotal",
            "amount",
            "final_amount",
            "invoice_total",
            "invoiceTotal",
        ],
    ));

    let total_amount = total_opt.unwrap_or_else(|| subtotal + tax);

    Ok(InvoiceData {
        invoice_id,
        vendor_name,
        invoice_date,
        currency,
        items,
        subtotal,
        tax,
        total_amount,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_bom_and_markdown() {
        let raw = "\u{FEFF}```json\n{\"id\": \"INV-123\", \"vendor\": \"Acme Corp\", \"total\": 500}\n```";
        let cleaned = clean_json_text(raw);
        assert_eq!(
            cleaned,
            "{\"id\": \"INV-123\", \"vendor\": \"Acme Corp\", \"total\": 500}"
        );
    }

    #[test]
    fn test_parse_camel_case_invoice() {
        let raw = r#"{
            "invoiceId": "INV-CAMEL-001",
            "vendorName": "Stripe Payments Ltd",
            "invoiceDate": "2026-10-24",
            "currencyCode": "USD",
            "lineItems": [
                {
                    "item": "Card Processing Fee",
                    "qty": "100",
                    "unitPrice": "$2.50",
                    "lineTotal": "250.00"
                }
            ],
            "subTotal": "$250.00",
            "taxAmount": "0.00",
            "totalAmount": "$250.00"
        }"#;

        let res = parse_flexible_invoice(raw).expect("Should parse camelCase invoice");
        assert_eq!(res.invoice_id, "INV-CAMEL-001");
        assert_eq!(res.vendor_name, "Stripe Payments Ltd");
        assert_eq!(res.subtotal, 250.0);
        assert_eq!(res.total_amount, 250.0);
        assert_eq!(res.items.len(), 1);
        assert_eq!(res.items[0].quantity, 100.0);
        assert_eq!(res.items[0].unit_price, 2.5);
    }

    #[test]
    fn test_parse_wrapped_invoice() {
        let raw = r#"{
            "status": "success",
            "data": {
                "invoice": {
                    "id": "INV-WRAP-999",
                    "supplier": "AWS Cloud",
                    "date": "2026-10-24",
                    "items": [
                        {"desc": "EC2 Instances", "qty": 1, "price": 400.0}
                    ],
                    "total": 400.0
                }
            }
        }"#;

        let res = parse_flexible_invoice(raw).expect("Should unwrap nested structure");
        assert_eq!(res.invoice_id, "INV-WRAP-999");
        assert_eq!(res.vendor_name, "AWS Cloud");
        assert_eq!(res.total_amount, 400.0);
        assert_eq!(res.subtotal, 400.0);
    }

    #[test]
    fn test_inr_currency_symbol_extraction() {
        let raw = r#"{
            "id": "INV-INR-SYM",
            "vendor": "Zomato Ltd.",
            "subtotal": "₹ 1,500.00",
            "tax": "₹ 75.00",
            "total": "₹ 1,575.00"
        }"#;

        let res = parse_flexible_invoice(raw).expect("Should parse INR with symbols");
        assert_eq!(res.currency, "INR");
        assert_eq!(res.subtotal, 1500.0);
        assert_eq!(res.tax, 75.0);
        assert_eq!(res.total_amount, 1575.0);
    }
}
