export type RiskLevel = 'LOW' | 'MEDIUM' | 'HIGH';
export type AuditStatus = 'PASSED' | 'REVIEW_RECOMMENDED' | 'ACTION_REQUIRED';

export interface LineItem {
  description: string;
  quantity: number;
  unit_price: number;
  total: number;
}

export interface InvoiceData {
  invoice_id: string;
  vendor_name: string;
  invoice_date: string;
  currency: string;
  items: LineItem[];
  subtotal: number;
  tax: number;
  total_amount: number;
}

export interface AuditFlag {
  rule_id: string;
  title: string;
  description: string;
  risk_level: RiskLevel;
}

export interface ExtractionMetadata {
  method: string;
  model: string;
  confidence: string;
  notes: string;
  derived_fields: string[];
}

export interface AuditReport {
  invoice_id: string;
  vendor_name: string;
  overall_status: AuditStatus;
  total_flags: number;
  high_risk_count: number;
  medium_risk_count: number;
  low_risk_count: number;
  flags: AuditFlag[];
  parsed_invoice: InvoiceData;
  timestamp: string;
  extraction?: ExtractionMetadata;
}

export interface BackendHealth {
  status: string;
  service: string;
  version: string;
  has_server_key: boolean;
  features?: {
    gemma_vision: boolean;
    streaming_chat: boolean;
    resilient_parser: boolean;
    digital_pdf_reader: boolean;
  };
}

export type KeyMode = 'SERVER' | 'BROWSER' | 'NATIVE';
