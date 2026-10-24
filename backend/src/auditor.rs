use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuditStatus {
    Passed,
    ReviewRecommended,
    ActionRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LineItem {
    pub description: String,
    pub quantity: f64,
    pub unit_price: f64,
    pub total: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InvoiceData {
    pub invoice_id: String,
    pub vendor_name: String,
    pub invoice_date: String,
    #[serde(default = "default_currency")]
    pub currency: String,
    #[serde(default)]
    pub items: Vec<LineItem>,
    pub subtotal: f64,
    pub tax: f64,
    pub total_amount: f64,
}

fn default_currency() -> String {
    "USD".to_string()
}

pub fn normalize_currency(currency_str: &str) -> String {
    match currency_str.trim().to_uppercase().as_str() {
        "INR" | "RS" | "RS." | "₹" => "INR".to_string(),
        "USD" | "$" => "USD".to_string(),
        other if !other.is_empty() => other.to_uppercase(),
        _ => default_currency(),
    }
}

pub const HIGH_VALUE_THRESHOLD_USD: f64 = 10_000.0;
pub const HIGH_VALUE_THRESHOLD_INR: f64 = 800_000.0; // ₹8,00,000 (8 Lakhs, ~10,000 USD equivalent)

pub fn get_currency_threshold(currency: &str) -> (f64, String) {
    let norm = normalize_currency(currency);
    if norm == "INR" {
        (HIGH_VALUE_THRESHOLD_INR, "INR".to_string())
    } else {
        (HIGH_VALUE_THRESHOLD_USD, norm)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditFlag {
    pub rule_id: String,
    pub title: String,
    pub description: String,
    pub risk_level: RiskLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditReport {
    pub invoice_id: String,
    pub vendor_name: String,
    pub overall_status: AuditStatus,
    pub total_flags: usize,
    pub high_risk_count: usize,
    pub medium_risk_count: usize,
    pub low_risk_count: usize,
    pub flags: Vec<AuditFlag>,
    pub parsed_invoice: InvoiceData,
    pub timestamp: String,
}

const FLOAT_EPSILON: f64 = 0.01;

pub fn audit_invoice(invoice: &InvoiceData) -> AuditReport {
    let mut flags: Vec<AuditFlag> = Vec::new();

    // Rule 1: Missing line items check
    if invoice.items.is_empty() {
        flags.push(AuditFlag {
            rule_id: "RULE_001_EMPTY_ITEMS".to_string(),
            title: "Missing Line Items".to_string(),
            description: "The invoice does not contain any itemized billing records or line items."
                .to_string(),
            risk_level: RiskLevel::High,
        });
    }

    // Rule 2: Line items sum vs. reported subtotal mismatch & per-item math check
    let mut calculated_subtotal = 0.0;
    for (idx, item) in invoice.items.iter().enumerate() {
        let expected_line_total = item.quantity * item.unit_price;
        if (item.total - expected_line_total).abs() > FLOAT_EPSILON {
            flags.push(AuditFlag {
                rule_id: format!("RULE_002_ITEM_CALC_MISMATCH_{}", idx + 1),
                title: format!("Line Item #{} Calculation Error", idx + 1),
                description: format!(
                    "Item '{}' reports total {:.2}, but quantity ({:.2}) * unit_price ({:.2}) equals {:.2}.",
                    item.description, item.total, item.quantity, item.unit_price, expected_line_total
                ),
                risk_level: RiskLevel::Medium,
            });
        }
        calculated_subtotal += item.total;
    }

    if !invoice.items.is_empty() && (calculated_subtotal - invoice.subtotal).abs() > FLOAT_EPSILON {
        flags.push(AuditFlag {
            rule_id: "RULE_002_SUBTOTAL_MISMATCH".to_string(),
            title: "Subtotal Discrepancy".to_string(),
            description: format!(
                "Calculated sum of line items ({:.2}) does not match declared subtotal ({:.2}). Discrepancy: {:.2}",
                calculated_subtotal,
                invoice.subtotal,
                (calculated_subtotal - invoice.subtotal).abs()
            ),
            risk_level: RiskLevel::High,
        });
    }

    // Rule 3: Subtotal + Tax vs. reported total amount mismatch
    let expected_total = invoice.subtotal + invoice.tax;
    if (expected_total - invoice.total_amount).abs() > FLOAT_EPSILON {
        flags.push(AuditFlag {
            rule_id: "RULE_003_TOTAL_MISMATCH".to_string(),
            title: "Total Amount Discrepancy".to_string(),
            description: format!(
                "Reported total ({:.2}) does not equal Subtotal ({:.2}) + Tax ({:.2}) = {:.2}. Discrepancy: {:.2}",
                invoice.total_amount,
                invoice.subtotal,
                invoice.tax,
                expected_total,
                (expected_total - invoice.total_amount).abs()
            ),
            risk_level: RiskLevel::High,
        });
    }

    // Rule 4: High-value transaction threshold check (currency-aware for USD and INR)
    let (currency_threshold, normalized_currency) = get_currency_threshold(&invoice.currency);
    if invoice.total_amount >= currency_threshold {
        flags.push(AuditFlag {
            rule_id: "RULE_004_HIGH_VALUE_THRESHOLD".to_string(),
            title: "High-Value Transaction".to_string(),
            description: format!(
                "Total invoice amount of {:.2} {} meets or exceeds the corporate risk audit threshold of {:.2} {}.",
                invoice.total_amount, normalized_currency, currency_threshold, normalized_currency
            ),
            risk_level: RiskLevel::Medium,
        });
    }

    // Rule 5: Empty or suspicious vendor check
    let trimmed_vendor = invoice.vendor_name.trim();
    let lower_vendor = trimmed_vendor.to_lowercase();
    let suspicious_names = [
        "n/a",
        "na",
        "unknown",
        "test",
        "test vendor",
        "dummy",
        "placeholder",
        "null",
        "none",
        "sample vendor",
    ];

    if trimmed_vendor.is_empty() || trimmed_vendor.len() < 2 {
        flags.push(AuditFlag {
            rule_id: "RULE_005_INVALID_VENDOR".to_string(),
            title: "Missing or Invalid Vendor".to_string(),
            description: "The vendor name is either empty, blank, or abnormally short.".to_string(),
            risk_level: RiskLevel::High,
        });
    } else if suspicious_names.contains(&lower_vendor.as_str()) {
        flags.push(AuditFlag {
            rule_id: "RULE_005_SUSPICIOUS_VENDOR".to_string(),
            title: "Suspicious Vendor Name".to_string(),
            description: format!(
                "The vendor name '{}' matches known placeholder or test patterns requiring vendor verification.",
                invoice.vendor_name
            ),
            risk_level: RiskLevel::High,
        });
    }

    // Optional Check: Negative pricing or tax
    if invoice.subtotal < 0.0 || invoice.total_amount < 0.0 || invoice.tax < 0.0 {
        flags.push(AuditFlag {
            rule_id: "RULE_006_NEGATIVE_AMOUNT".to_string(),
            title: "Negative Value Detected".to_string(),
            description: "Subtotal, tax, or total amount is negative without credit memo indication."
                .to_string(),
            risk_level: RiskLevel::Medium,
        });
    }

    // Count severities
    let high_risk_count = flags
        .iter()
        .filter(|f| f.risk_level == RiskLevel::High)
        .count();
    let medium_risk_count = flags
        .iter()
        .filter(|f| f.risk_level == RiskLevel::Medium)
        .count();
    let low_risk_count = flags
        .iter()
        .filter(|f| f.risk_level == RiskLevel::Low)
        .count();
    let total_flags = flags.len();

    // Determine overall audit status
    let overall_status = if high_risk_count > 0 {
        AuditStatus::ActionRequired
    } else if medium_risk_count > 0 || low_risk_count >= 2 {
        AuditStatus::ReviewRecommended
    } else {
        AuditStatus::Passed
    };

    AuditReport {
        invoice_id: invoice.invoice_id.clone(),
        vendor_name: invoice.vendor_name.clone(),
        overall_status,
        total_flags,
        high_risk_count,
        medium_risk_count,
        low_risk_count,
        flags,
        parsed_invoice: invoice.clone(),
        timestamp: Utc::now().to_rfc3339(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_valid_invoice() -> InvoiceData {
        InvoiceData {
            invoice_id: "INV-2026-001".to_string(),
            vendor_name: "Acme Cloud Services Inc.".to_string(),
            invoice_date: "2026-10-24".to_string(),
            currency: "USD".to_string(),
            items: vec![
                LineItem {
                    description: "Dedicated Server Instance (Monthly)".to_string(),
                    quantity: 2.0,
                    unit_price: 250.0,
                    total: 500.0,
                },
                LineItem {
                    description: "Managed Backup Storage (TB)".to_string(),
                    quantity: 4.0,
                    unit_price: 50.0,
                    total: 200.0,
                },
            ],
            subtotal: 700.0,
            tax: 70.0,
            total_amount: 770.0,
        }
    }

    #[test]
    fn test_clean_invoice_passes() {
        let invoice = create_valid_invoice();
        let report = audit_invoice(&invoice);

        assert_eq!(report.overall_status, AuditStatus::Passed);
        assert_eq!(report.total_flags, 0);
        assert_eq!(report.high_risk_count, 0);
        assert_eq!(report.medium_risk_count, 0);
    }

    #[test]
    fn test_subtotal_mismatch_fails() {
        let mut invoice = create_valid_invoice();
        invoice.subtotal = 850.0; // Incorrect subtotal (items sum is 700)
        let report = audit_invoice(&invoice);

        assert_eq!(report.overall_status, AuditStatus::ActionRequired);
        assert!(report
            .flags
            .iter()
            .any(|f| f.rule_id == "RULE_002_SUBTOTAL_MISMATCH"));
    }

    #[test]
    fn test_total_mismatch_fails() {
        let mut invoice = create_valid_invoice();
        invoice.total_amount = 999.0; // 700 + 70 != 999
        let report = audit_invoice(&invoice);

        assert_eq!(report.overall_status, AuditStatus::ActionRequired);
        assert!(report
            .flags
            .iter()
            .any(|f| f.rule_id == "RULE_003_TOTAL_MISMATCH"));
    }

    #[test]
    fn test_high_value_transaction_warning() {
        let mut invoice = create_valid_invoice();
        invoice.items = vec![LineItem {
            description: "Enterprise Architecture Retainer".to_string(),
            quantity: 1.0,
            unit_price: 15_000.0,
            total: 15_000.0,
        }];
        invoice.subtotal = 15_000.0;
        invoice.tax = 1_500.0;
        invoice.total_amount = 16_500.0;

        let report = audit_invoice(&invoice);
        assert_eq!(report.overall_status, AuditStatus::ReviewRecommended);
        assert_eq!(report.medium_risk_count, 1);
        assert!(report
            .flags
            .iter()
            .any(|f| f.rule_id == "RULE_004_HIGH_VALUE_THRESHOLD"));
    }

    #[test]
    fn test_suspicious_vendor_flagged() {
        let mut invoice = create_valid_invoice();
        invoice.vendor_name = "N/A".to_string();

        let report = audit_invoice(&invoice);
        assert_eq!(report.overall_status, AuditStatus::ActionRequired);
        assert!(report
            .flags
            .iter()
            .any(|f| f.rule_id == "RULE_005_SUSPICIOUS_VENDOR"));
    }

    #[test]
    fn test_missing_line_items() {
        let mut invoice = create_valid_invoice();
        invoice.items.clear();
        invoice.subtotal = 0.0;
        invoice.tax = 0.0;
        invoice.total_amount = 0.0;

        let report = audit_invoice(&invoice);
        assert_eq!(report.overall_status, AuditStatus::ActionRequired);
        assert!(report
            .flags
            .iter()
            .any(|f| f.rule_id == "RULE_001_EMPTY_ITEMS"));
    }

    #[test]
    fn test_inr_currency_normalization() {
        assert_eq!(normalize_currency("INR"), "INR");
        assert_eq!(normalize_currency("inr"), "INR");
        assert_eq!(normalize_currency("Rs"), "INR");
        assert_eq!(normalize_currency("₹"), "INR");
        assert_eq!(normalize_currency("usd"), "USD");
        assert_eq!(normalize_currency("$"), "USD");
    }

    #[test]
    fn test_inr_clean_invoice_passes() {
        let invoice = InvoiceData {
            invoice_id: "INV-INR-2026-101".to_string(),
            vendor_name: "Tata Consultancy & Cloud Ltd.".to_string(),
            invoice_date: "2026-10-24".to_string(),
            currency: "INR".to_string(),
            items: vec![
                LineItem {
                    description: "Managed Kubernetes Cluster".to_string(),
                    quantity: 1.0,
                    unit_price: 150_000.0,
                    total: 150_000.0,
                },
                LineItem {
                    description: "High IOPS Database Volumes".to_string(),
                    quantity: 2.0,
                    unit_price: 25_000.0,
                    total: 50_000.0,
                },
            ],
            subtotal: 200_000.0,
            tax: 36_000.0, // 18% GST
            total_amount: 236_000.0,
        };

        let report = audit_invoice(&invoice);
        assert_eq!(report.overall_status, AuditStatus::Passed);
        assert_eq!(report.total_flags, 0);
    }

    #[test]
    fn test_inr_high_value_threshold_triggers() {
        let invoice = InvoiceData {
            invoice_id: "INV-INR-HIGH-002".to_string(),
            vendor_name: "Infosys Infrastructure Services".to_string(),
            invoice_date: "2026-10-24".to_string(),
            currency: "INR".to_string(),
            items: vec![LineItem {
                description: "Annual Enterprise Support Contract".to_string(),
                quantity: 1.0,
                unit_price: 850_000.0,
                total: 850_000.0,
            }],
            subtotal: 850_000.0,
            tax: 153_000.0,
            total_amount: 1_003_000.0,
        };

        let report = audit_invoice(&invoice);
        assert_eq!(report.overall_status, AuditStatus::ReviewRecommended);
        assert!(report
            .flags
            .iter()
            .any(|f| f.rule_id == "RULE_004_HIGH_VALUE_THRESHOLD"));
    }
}
