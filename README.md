# DocuAudit • Intelligent Document & Invoice Auditor Web Application

An intelligent, corporate-grade document and invoice auditor web application featuring a high-performance **Rust (Axum + Tokio)** backend and a modern **React 19 + Tailwind CSS** dark-themed frontend with **SmoothUI** AI components.

---

## Architecture Overview

```
                      +---------------------------------------+
                      |       Modern Dark UI (React 19)       |
                      |     (Vite + Tailwind + SmoothUI)      |
                      |         http://localhost:5173         |
                      +-------------------+-------------------+
                                          |
                +-------------------------+-------------------------+
                |                         |                         |
         (Health Polling)          (Auditing & Upload)       (Streaming SSE Chat)
           GET /health             POST /api/audit-json       POST /api/chat
                |                  POST /api/upload                 |
                |                  POST /api/extract                |
                +-------------------------+-------------------------+
                                          |
                      +-------------------v-------------------+
                      |            Backend Engine             |
                      |         (Rust + Axum + Tokio)         |
                      |         http://localhost:3000         |
                      +-------------------+-------------------+
                                          |
         +--------------------------------+--------------------------------+
         |                                |                                |
+--------v--------+             +---------v---------+            +---------v---------+
|  Auditor Engine |             |  Resilient Parser |            |  Multimodal Gemma |
|  (auditor.rs)   |             |    (parser.rs)    |            | (pdf_scanner.rs)  |
| 5 Audit Rules   |             | Tracks Derived &  |            | Verbatim Vision   |
| Status & Risks  |             | Defaulted Fields  |            | & Digital PDF OCR |
+-----------------+             +-------------------+            +-------------------+
```

---

## Key Features

### 1. Corporate Auditing Rules (5 Comprehensive Rules)
- **`RULE_001_EMPTY_ITEMS`**: Detects empty or missing itemized billing line items (**High Risk**).
- **`RULE_002_SUBTOTAL_MISMATCH`**: Validates line item math (`quantity * unit_price == total`) and ensures the sum of line items equals the declared subtotal (**High/Medium Risk**).
- **`RULE_003_TOTAL_MISMATCH`**: Verifies `subtotal + tax == total_amount` (**High Risk**).
- **`RULE_004_HIGH_VALUE_THRESHOLD`**: Currency-aware risk threshold detection (**Medium Risk**):
  - **USD**: $\ge \$10,000.00$
  - **INR**: $\ge \text{₹}8,00,000.00$ (8 Lakhs, $\approx \$10,000$ USD equivalent)
  - Automatic detection of `$` and `₹` symbols.
- **`RULE_005_SUSPICIOUS_VENDOR`**: Flags missing, abnormally short, or placeholder vendor identities (`"N/A"`, `"Unknown"`, `"Test"`, `"Test Vendor"`, `"Dummy"`, `"Sample Vendor"`) (**High Risk**).
- **Overall Status Assessment**:
  - `PASSED`: Zero high or medium risk flags.
  - `REVIEW_RECOMMENDED`: Flagged for review with medium risk items.
  - `ACTION_REQUIRED`: High-risk discrepancies requiring immediate auditor action.
  - High, Medium, and Low risk counts with detailed calculation explanations.

### 2. Reading Inputs & Extraction
- **Schema-Valid JSON**: Audited untouched directly against the schema with zero AI needed.
- **Messy JSON Normalizer**: Built-in resilient parser maps camelCase, aliased keys, wrapped container objects, numbers as strings with currency symbols (`₹`, `$`, commas), and markdown fences without requiring AI.
- **Audit Transparency**: **Every field the parser had to default or derive is listed in `derived_fields`**, so nothing is silently "fixed".
- **Digital PDFs**: Read natively from embedded text layers (`pdf-extract`), with or without an AI key.
- **Scanned PDFs**: Rendered to page images in the browser (up to 5 pages) and transcribed by Gemma.
- **Photos & Scans**: Uploaded images (PNG, JPG, WEBP) are upscaled up to 4x, contrast-stretched, and sharpened before Gemma reads them.
- **Multi-Page Image Invoices**: Up to 8 images are treated as consecutive pages of a single invoice.
- **Verbatim Rule**: Gemma only transcribes and **never recomputes amounts**, preserving arithmetic errors so they are caught by the auditor.
- **Metadata**: Each extraction displays its extraction method, model, confidence level, and operational notes.

### 3. AI Features (Powered by Gemma / Gemini)
- **Streaming Chat Assistant**: Real-time SSE streaming copilot (`/api/chat`) that knows your current audit report findings, invoice figures, and flagged discrepancies.
- **Suggested Questions**: Contextual prompt chips for instant auditing inquiries.
- **Free-Form Text Extraction**: Unstructured text and emails are mapped into structured invoices by Gemma.
- **Key Flexibility**: API keys can be set in `backend/.env` or in the browser's **Gemma AI** settings dialog.
- **Key Status Badge**: Shows `Server key`, `Browser key`, or `Native mode`.
- **Offline Capable**: Without an API key, JSON, messy JSON, and digital PDFs function entirely offline.

### 4. Modern Dark UI with SmoothUI Components
- **Framework**: React 19 + Tailwind CSS + Lucide Icons + Motion.
- **SmoothUI AI Components**:
  - `@smoothui/agent-avatar`: Animated seeded generative pixel avatar for the auditor assistant.
  - `@smoothui/ai-conversation`: Pinned scroll thread tracking streamed responses.
  - `@smoothui/ai-message`: Polished chat message bubbles with copy and action states.
  - `@smoothui/ai-response`: Word-by-word streaming text animation with animated caret.
  - `@smoothui/ai-loader`: Smooth waiting and thinking indicators.
  - `@smoothui/ai-suggestions`: Radiating suggestion chips for prompt suggestions.
  - `@smoothui/ai-prompt-input`: Auto-expanding composer with send/stop toggling.
  - `@smoothui/ai-context-meter`: Visual token context meter for active audit reports.
  - `@smoothui/ai-approval`: Human-in-the-loop decision card for audit approvals.
  - `@smoothui/ai-artifact`: Frame for previewing generated vendor dispute emails.
  - `@smoothui/ai-citation`: Interactive citation pills linking to audit rules.
  - `@smoothui/ai-sources`: Context source cards linking to document data.
- **Cross-Device Ready**: The Rust backend serves the built React UI on fallback, allowing access from phones and tablets on the same network.

---

## API Endpoints

| Method | Path | Description |
|---|---|---|
| `GET` | `/health` | Health check reporting service status, version, and server key presence |
| `POST` | `/api/audit-json` | Audits raw schema-valid or messy JSON text with derivation logging |
| `POST` | `/api/upload` | Multipart file upload for digital PDFs, scanned PDFs, images, and JSON |
| `POST` | `/api/extract` | Multimodal Gemma extraction for multi-page images (up to 8) or free-form text |
| `POST` | `/api/chat` | Streaming SSE chat assistant aware of active audit report context |

---

## Running the Project

### 1. Build the Frontend
```bash
cd frontend
pnpm install
pnpm build
```

### 2. Start the Backend Server
```bash
cd backend
# Optional: create backend/.env with GEMINI_API_KEY="your-key"
cargo run
```

Open `http://localhost:3000` in your browser.
To run automated Rust unit tests:
```bash
cd backend
cargo test
```
