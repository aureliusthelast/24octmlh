# DocuAudit • Document & Invoice Auditor Web Application

An intelligent, high-performance, and modular local document and invoice auditor web application featuring a **Rust (Axum + Tokio)** backend and a modern **TypeScript (Vite)** dark-themed frontend.

---

## Architecture Overview

```
                      +-----------------------------+
                      |       Modern Frontend       |
                      |   (TypeScript + Vite)       |
                      |   http://localhost:5173     |
                      +--------------+--------------+
                                     |
               +---------------------+---------------------+
               |                     |                     |
        (Health Polling)       (JSON Auditing)      (File Upload)
         GET /health        POST /api/audit-json   POST /api/upload
               |                     |                     |
               +---------------------+---------------------+
                                     |
                      +--------------v--------------+
                      |       Backend Engine        |
                      |    (Rust + Axum + Tokio)    |
                      |    http://localhost:3000    |
                      +--------------+--------------+
                                     |
                      +--------------v--------------+
                      |    Auditor Engine Module    |
                      |        (auditor.rs)         |
                      |  - Line items sum check     |
                      |  - Tax & total check        |
                      |  - High-value threshold     |
                      |  - Vendor credibility check |
                      |  - Empty items check        |
                      +-----------------------------+
```

---

## Features

- **Blazing Fast Rust Backend**:
  - Built with **Axum 0.7**, **Tokio**, and **Tower-HTTP CORS**.
  - Strongly typed data validation and arithmetic discrepancy detection.
  - Dedicated endpoints for health checks (`GET /health`), raw JSON analysis (`POST /api/audit-json`), and multipart file uploads (`POST /api/upload`).
- **5 Comprehensive Verification Rules**:
  1. `RULE_001_EMPTY_ITEMS`: Verifies the invoice contains non-empty line items.
  2. `RULE_002_SUBTOTAL_MISMATCH`: Verifies line items' calculations (`quantity * unit_price == total`) and that the item sum equals declared subtotal.
  3. `RULE_003_TOTAL_MISMATCH`: Confirms `subtotal + tax == total_amount`.
  4. `RULE_004_HIGH_VALUE_THRESHOLD`: Identifies high-value invoices ($\ge \$10,000$) for dual-authorization approval.
  5. `RULE_005_SUSPICIOUS_VENDOR`: Flags blank, missing, placeholder (`"N/A"`, `"Unknown"`, `"Test Vendor"`), or invalid vendor identities.
- **Modern Responsive Dark UI**:
  - Glassmorphic card styling, responsive CSS grid/flexbox, and JetBrains Mono code typography.
  - Drag-and-drop file upload target for `.json` and text files.
  - One-click **"⚡ Test Clean Sample"** and **"⚠️ Test Faulty Sample"** buttons for immediate demonstration.
  - Live server status indicator with automated ping and reload triggers.
  - Split-screen comparison view:
    - **Left pane**: Clean, syntax-highlighted JSON viewer with one-click clipboard copy.
    - **Right pane**: Metric counters for High, Medium, and Low risks, alongside itemized color-coded findings.

---

## Directory Structure

```
.
├── backend/
│   ├── Cargo.toml          # Rust dependencies & crate configuration
│   └── src/
│       ├── main.rs         # Axum server, CORS, route handlers
│       └── auditor.rs      # Core domain models, audit rules & unit tests
├── frontend/
│   ├── package.json        # Frontend scripts and Vite/TypeScript devDependencies
│   ├── tsconfig.json       # Strict TypeScript configuration
│   ├── index.html          # Semantic HTML layout and UI components
│   ├── style.css           # Modern dark-theme stylesheet
│   └── src/
│       └── main.ts         # TypeScript app logic, API clients & DOM renderer
├── samples/
│   ├── clean_invoice.json  # Valid test invoice passing all checks
│   └── faulty_invoice.json # Deliberately malformed invoice triggering flags
└── README.md
```

---

## Prerequisites

- **Rust & Cargo** (v1.75+ or newer)
- **Node.js** (v18+ or newer) and **npm**

---

## Running the Application

### 1. Start the Rust Backend

Open a terminal and run:

```bash
cd backend
cargo run
```

The backend server will start on `http://localhost:3000`.

To run the automated Rust test suite:
```bash
cargo test
```

### 2. Start the Frontend

Open a second terminal and run:

```bash
cd frontend
npm install
npm run dev
```

Open your browser and navigate to:
```
http://localhost:5173
```

---

## API Endpoints

### 1. Health Status
- **Method**: `GET`
- **Route**: `/health`
- **Response**:
```json
{
  "service": "invoice-auditor",
  "status": "ok",
  "version": "0.1.0"
}
```

### 2. Direct JSON Audit
- **Method**: `POST`
- **Route**: `/api/audit-json`
- **Headers**: `Content-Type: application/json`
- **Request Body**:
```json
{
  "invoice_id": "INV-2026-001",
  "vendor_name": "Acme Cloud Services Inc.",
  "invoice_date": "2026-10-24",
  "currency": "USD",
  "items": [
    {
      "description": "Server Instance",
      "quantity": 2.0,
      "unit_price": 250.0,
      "total": 500.0
    }
  ],
  "subtotal": 500.0,
  "tax": 50.0,
  "total_amount": 550.0
}
```
- **Response**: `AuditReport` JSON with `overall_status`, flag counts, and list of flagged issues.

### 3. File Upload Audit
- **Method**: `POST`
- **Route**: `/api/upload`
- **Headers**: `multipart/form-data`
- **Form Key**: `file` (a `.json` or text file)
- **Response**: `AuditReport` JSON.

---

## License

This project is licensed under the [MIT License](https://opensource.org/licenses/MIT).
