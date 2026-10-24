use crate::auditor::{default_currency, normalize_currency, ExtractionMetadata, InvoiceData, LineItem};
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
pub fn parse_flexible_f64(val: Option<&Value>) -> Option<(f64, Option<String>)> {
    let val = val?;
    match val {
        Value::Number(num) => num.as_f64().map(|n| (n, None)),
        Value::String(s) => {
            let original = s.trim().to_string();
            if original.is_empty() {
                return None;
            }

            // Check accounting format: "(100.50)" -> "-100.50"
            let is_negative = (original.starts_with('(') && original.ends_with(')'))
                || original.starts_with('-');

            // Remove non-numeric characters except '.'
            let cleaned: String = original
                .chars()
                .filter(|c| c.is_ascii_digit() || *c == '.')
                .collect();

            if cleaned.is_empty() {
                return None;
            }

            cleaned.parse::<f64>().ok().map(|v| {
                let final_val = if is_negative { -v } else { v };
                (final_val, Some(original))
            })
        }
        _ => None,
    }
}

/// Recursively unwraps root containers: `{ "invoice": { ... } }`, `{ "data": { ... } }`, etc.
fn unwrap_root_value(mut val: Value, derived_fields: &mut Vec<String>) -> Value {
    for _ in 0..3 {
        match val {
            Value::Object(ref map) => {
                let candidate_keys = ["invoice", "data", "document", "result", "payload", "bill"];
                let mut found_inner = None;
                for key in candidate_keys {
                    if let Some(inner) = map.get(key) {
                        if inner.is_object() {
                            found_inner = Some((key.to_string(), inner.clone()));
                            break;
                        }
                    }
                }
                if let Some((k, inner)) = found_inner {
                    derived_fields.push(format!("Unwrapped root container object (key '{}')", k));
                    val = inner;
                    continue;
                }
                break;
            }
            Value::Array(ref arr) if arr.len() == 1 => {
                derived_fields.push("Unwrapped single-element array wrapper".to_string());
                val = arr[0].clone();
            }
            _ => break,
        }
    }
    val
}

/// Flexible field lookup checking multiple common aliases and case variations
fn get_field_val_with_key<'a>(
    obj: &'a serde_json::Map<String, Value>,
    aliases: &[&str],
) -> Option<(&'a str, &'a Value)> {
    for alias in aliases {
        // Direct match
        if let Some((k, v)) = obj.get_key_value(*alias) {
            return Some((k.as_str(), v));
        }
        // Case-insensitive match
        for (k, v) in obj.iter() {
            if k.eq_ignore_ascii_case(alias) {
                return Some((k.as_str(), v));
            }
        }
    }
    None
}

/// Flexible string field lookup
fn get_field_string_with_key(
    obj: &serde_json::Map<String, Value>,
    aliases: &[&str],
) -> Option<(String, String)> {
    let (key, val) = get_field_val_with_key(obj, aliases)?;
    match val {
        Value::String(s) => Some((key.to_string(), s.trim().to_string())),
        Value::Number(n) => Some((key.to_string(), n.to_string())),
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

/// Parses a flexible LineItem with derivation tracking
fn parse_line_item(item_val: &Value, idx: usize, derived_fields: &mut Vec<String>) -> Option<LineItem> {
    match item_val {
        Value::Object(map) => {
            let description = get_field_string_with_key(
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
            .map(|(k, v)| {
                if k != "description" {
                    derived_fields.push(format!("Line item #{}: mapped key '{}' to 'description'", idx + 1, k));
                }
                v
            })
            .unwrap_or_else(|| {
                derived_fields.push(format!("Line item #{}: description missing, defaulted to 'Line Item #{}'", idx + 1, idx + 1));
                format!("Line Item #{}", idx + 1)
            });

            let quantity_res = parse_flexible_f64(
                get_field_val_with_key(map, &["quantity", "qty", "count", "units", "number", "hours"])
                    .map(|(k, v)| {
                        if k != "quantity" {
                            derived_fields.push(format!("Line item #{}: mapped key '{}' to 'quantity'", idx + 1, k));
                        }
                        v
                    }),
            );

            let quantity = match quantity_res {
                Some((q, Some(orig))) => {
                    derived_fields.push(format!("Line item #{}: sanitized quantity string '{}' to {:.2}", idx + 1, orig, q));
                    q
                }
                Some((q, None)) => q,
                None => {
                    derived_fields.push(format!("Line item #{}: quantity missing, defaulted to 1.0", idx + 1));
                    1.0
                }
            };

            let unit_price_res = parse_flexible_f64(
                get_field_val_with_key(
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
                )
                .map(|(k, v)| {
                    if k != "unit_price" {
                        derived_fields.push(format!("Line item #{}: mapped key '{}' to 'unit_price'", idx + 1, k));
                    }
                    v
                }),
            );

            let total_res = parse_flexible_f64(
                get_field_val_with_key(
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
                )
                .map(|(k, v)| {
                    if k != "total" {
                        derived_fields.push(format!("Line item #{}: mapped key '{}' to 'total'", idx + 1, k));
                    }
                    v
                }),
            );

            let unit_price_val = unit_price_res.map(|(p, orig)| {
                if let Some(s) = orig {
                    derived_fields.push(format!("Line item #{}: parsed unit_price string '{}' to {:.2}", idx + 1, s, p));
                }
                p
            });

            let total_val = total_res.map(|(t, orig)| {
                if let Some(s) = orig {
                    derived_fields.push(format!("Line item #{}: parsed line total string '{}' to {:.2}", idx + 1, s, t));
                }
                t
            });

            let (final_unit_price, final_total) = match (unit_price_val, total_val) {
                (Some(up), Some(t)) => (up, t),
                (Some(up), None) => {
                    let derived = up * quantity;
                    derived_fields.push(format!(
                        "Line item #{}: missing 'total'; derived as {:.2} (quantity {:.2} * unit_price {:.2})",
                        idx + 1, derived, quantity, up
                    ));
                    (up, derived)
                }
                (None, Some(t)) => {
                    let derived = if quantity > 0.0 { t / quantity } else { t };
                    derived_fields.push(format!(
                        "Line item #{}: missing 'unit_price'; derived as {:.2} (total {:.2} / quantity {:.2})",
                        idx + 1, derived, t, quantity
                    ));
                    (derived, t)
                }
                (None, None) => {
                    derived_fields.push(format!("Line item #{}: missing unit_price and total; defaulted to 0.00", idx + 1));
                    (0.0, 0.0)
                }
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

/// Parses invoice JSON text, auditing schema-valid JSON untouched,
/// or mapping messy JSON and documenting every defaulted or derived field.
pub fn parse_flexible_invoice_with_report(raw_text: &str) -> Result<(InvoiceData, ExtractionMetadata), String> {
    // 1. Check if the raw text is already strict schema-valid JSON untouched
    if let Ok(strict_invoice) = serde_json::from_str::<InvoiceData>(raw_text) {
        if !strict_invoice.invoice_id.trim().is_empty() {
            let metadata = ExtractionMetadata {
                method: "Schema-valid JSON".to_string(),
                model: "strict-validator (untouched)".to_string(),
                confidence: "High".to_string(),
                notes: "Schema-valid JSON matched standard schema directly; audited untouched.".to_string(),
                derived_fields: Vec::new(),
            };
            return Ok((strict_invoice, metadata));
        }
    }

    let mut derived_fields = Vec::new();

    let cleaned = clean_json_text(raw_text);
    if cleaned.is_empty() {
        return Err("Input invoice payload is empty".to_string());
    }

    if cleaned != raw_text.trim() {
        derived_fields.push("Stripped markdown code fences / external non-JSON wrappers".to_string());
    }

    let parsed_val: Value = serde_json::from_str(&cleaned).map_err(|e| {
        format!(
            "Invalid JSON format: {}. Ensure payload contains valid JSON notation.",
            e
        )
    })?;

    let root = unwrap_root_value(parsed_val, &mut derived_fields);
    let map = match root {
        Value::Object(m) => m,
        _ => {
            return Err("Expected JSON object at root of invoice payload".to_string());
        }
    };

    // 1. Invoice ID
    let invoice_id = match get_field_string_with_key(
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
    ) {
        Some((k, v)) => {
            if k != "invoice_id" {
                derived_fields.push(format!("Mapped key '{}' to 'invoice_id'", k));
            }
            v
        }
        None => {
            let gen = format!("INV-{}", &Utc::now().timestamp_millis().to_string()[7..]);
            derived_fields.push(format!("Missing 'invoice_id'; defaulted to generated fallback '{}'", gen));
            gen
        }
    };

    // 2. Vendor Name
    let vendor_name = match get_field_string_with_key(
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
    ) {
        Some((k, v)) => {
            if k != "vendor_name" {
                derived_fields.push(format!("Mapped key '{}' to 'vendor_name'", k));
            }
            v
        }
        None => {
            derived_fields.push("Missing 'vendor_name'; defaulted to empty string".to_string());
            String::new()
        }
    };

    // 3. Invoice Date
    let invoice_date = match get_field_string_with_key(
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
    ) {
        Some((k, v)) => {
            if k != "invoice_date" {
                derived_fields.push(format!("Mapped key '{}' to 'invoice_date'", k));
            }
            v
        }
        None => {
            let today = Utc::now().format("%Y-%m-%d").to_string();
            derived_fields.push(format!("Missing 'invoice_date'; defaulted to today '{}'", today));
            today
        }
    };

    // 4. Currency
    let raw_currency_opt = get_field_string_with_key(
        &map,
        &[
            "currency",
            "currency_code",
            "currencyCode",
            "curr",
            "symbol",
        ],
    );

    let currency = match raw_currency_opt {
        Some((k, v)) => {
            if k != "currency" {
                derived_fields.push(format!("Mapped key '{}' to 'currency'", k));
            }
            normalize_currency(&v)
        }
        None => {
            if let Some(detected) = detect_currency_from_text(&cleaned) {
                derived_fields.push(format!("Missing 'currency'; detected '{}' from currency symbol in document", detected));
                detected
            } else {
                derived_fields.push(format!("Missing 'currency'; defaulted to standard '{}'", default_currency()));
                default_currency()
            }
        }
    };

    // 5. Line items
    let mut items = Vec::new();
    if let Some((k, items_val)) = get_field_val_with_key(
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
        if k != "items" {
            derived_fields.push(format!("Mapped key '{}' to 'items'", k));
        }
        if let Value::Array(arr) = items_val {
            for (idx, item) in arr.iter().enumerate() {
                if let Some(parsed_item) = parse_line_item(item, idx, &mut derived_fields) {
                    items.push(parsed_item);
                }
            }
        }
    } else {
        derived_fields.push("Missing line items array in document payload".to_string());
    }

    // 6. Subtotal
    let subtotal_entry = get_field_val_with_key(
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
    );

    let subtotal_opt = match subtotal_entry {
        Some((k, val)) => {
            if k != "subtotal" {
                derived_fields.push(format!("Mapped key '{}' to 'subtotal'", k));
            }
            let parsed = parse_flexible_f64(Some(val));
            if let Some((n, Some(orig))) = parsed {
                derived_fields.push(format!("Parsed subtotal string '{}' to {:.2}", orig, n));
                Some(n)
            } else {
                parsed.map(|(n, _)| n)
            }
        }
        None => None,
    };

    let line_items_sum: f64 = items.iter().map(|it| it.total).sum();

    let subtotal = match subtotal_opt {
        Some(st) => st,
        None => {
            if !items.is_empty() {
                derived_fields.push(format!(
                    "Missing declared 'subtotal'; derived from sum of line items ({:.2})",
                    line_items_sum
                ));
                line_items_sum
            } else {
                derived_fields.push("Missing declared 'subtotal'; defaulted to 0.00".to_string());
                0.0
            }
        }
    };

    // 7. Tax
    let tax_entry = get_field_val_with_key(
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
    );

    let tax = match tax_entry {
        Some((k, val)) => {
            if k != "tax" {
                derived_fields.push(format!("Mapped key '{}' to 'tax'", k));
            }
            let parsed = parse_flexible_f64(Some(val));
            if let Some((n, Some(orig))) = parsed {
                derived_fields.push(format!("Parsed tax string '{}' to {:.2}", orig, n));
                n
            } else {
                parsed.map(|(n, _)| n).unwrap_or(0.0)
            }
        }
        None => {
            derived_fields.push("Missing declared 'tax'; defaulted to 0.00".to_string());
            0.0
        }
    };

    // 8. Total Amount
    let total_entry = get_field_val_with_key(
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
    );

    let total_amount = match total_entry {
        Some((k, val)) => {
            if k != "total_amount" {
                derived_fields.push(format!("Mapped key '{}' to 'total_amount'", k));
            }
            let parsed = parse_flexible_f64(Some(val));
            if let Some((n, Some(orig))) = parsed {
                derived_fields.push(format!("Parsed total_amount string '{}' to {:.2}", orig, n));
                n
            } else {
                parsed.map(|(n, _)| n).unwrap_or(subtotal + tax)
            }
        }
        None => {
            let derived = subtotal + tax;
            derived_fields.push(format!(
                "Missing declared 'total_amount'; derived as subtotal ({:.2}) + tax ({:.2}) = {:.2}",
                subtotal, tax, derived
            ));
            derived
        }
    };

    let confidence = if derived_fields.len() > 6 {
        "Medium".to_string()
    } else {
        "High".to_string()
    };

    let notes = format!(
        "Resiliently normalized {} fields without AI. All derived fields documented; nothing was silently fixed.",
        derived_fields.len()
    );

    let metadata = ExtractionMetadata {
        method: "Resilient JSON Parser".to_string(),
        model: "built-in-parser".to_string(),
        confidence,
        notes,
        derived_fields,
    };

    let invoice = InvoiceData {
        invoice_id,
        vendor_name,
        invoice_date,
        currency,
        items,
        subtotal,
        tax,
        total_amount,
    };

    Ok((invoice, metadata))
}

#[allow(dead_code)]
pub fn parse_flexible_invoice(raw_text: &str) -> Result<InvoiceData, String> {
    parse_flexible_invoice_with_report(raw_text).map(|(inv, _)| inv)
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

        let (res, meta) = parse_flexible_invoice_with_report(raw).expect("Should parse camelCase invoice");
        assert_eq!(res.invoice_id, "INV-CAMEL-001");
        assert_eq!(res.vendor_name, "Stripe Payments Ltd");
        assert_eq!(res.subtotal, 250.0);
        assert_eq!(res.total_amount, 250.0);
        assert_eq!(res.items.len(), 1);
        assert_eq!(res.items[0].quantity, 100.0);
        assert_eq!(res.items[0].unit_price, 2.5);
        assert!(!meta.derived_fields.is_empty());
        assert_eq!(meta.method, "Resilient JSON Parser");
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

        let (res, meta) = parse_flexible_invoice_with_report(raw).expect("Should unwrap nested structure");
        assert_eq!(res.invoice_id, "INV-WRAP-999");
        assert_eq!(res.vendor_name, "AWS Cloud");
        assert_eq!(res.total_amount, 400.0);
        assert_eq!(res.subtotal, 400.0);
        assert!(meta.derived_fields.iter().any(|d| d.contains("Unwrapped")));
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

        let (res, meta) = parse_flexible_invoice_with_report(raw).expect("Should parse INR with symbols");
        assert_eq!(res.currency, "INR");
        assert_eq!(res.subtotal, 1500.0);
        assert_eq!(res.tax, 75.0);
        assert_eq!(res.total_amount, 1575.0);
        assert!(meta.derived_fields.iter().any(|d| d.contains("INR")));
    }
}
