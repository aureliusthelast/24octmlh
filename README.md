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

## Directory Structure

```
.
├── backend/
│   ├── Cargo.toml          # Rust crate configuration & dependencies
│   ├── .env.example        # Optional environment variables (GEMINI_API_KEY, PORT, etc.)
│   └── src/
│       ├── main.rs         # Axum server, API route handlers, SSE chat stream & static UI serving
│       ├── auditor.rs      # Core domain models, 5 audit rules & unit tests
│       ├── parser.rs       # Resilient JSON parser (normalizes messy keys, wrappers & currency)
│       └── pdf_scanner.rs  # Native PDF text extractor & Gemma multimodal vision OCR
├── frontend/
│   ├── package.json        # Frontend scripts and React 19 dependencies
│   ├── vite.config.ts      # Vite bundler configuration
│   ├── tailwind.config.js  # Tailwind CSS configuration
│   ├── index.html          # HTML entry point
│   └── src/
│       ├── App.tsx         # Main application layout, audit viewer & AI chat drawer
│       ├── components/     # SmoothUI AI components (chat, avatars, approvals, artifacts)
│       ├── lib/            # Image preprocessing pipeline (canvas 4x upscale, contrast, sharpening)
│       ├── samples.ts      # Built-in one-click test sample fixtures
│       └── types.ts        # Shared TypeScript data models & audit interfaces
├── samples/
│   ├── clean_invoice.json  # Valid USD invoice passing all rules
│   ├── faulty_invoice.json # Deliberately malformed invoice triggering audit flags
│   ├── inr_invoice.json    # Valid INR invoice testing Indian currency threshold
│   ├── messy_invoice.json  # Resilient parser test fixture (messy keys & strings)
│   └── sample_invoice.pdf  # Sample PDF invoice for native & OCR scanning
└── README.md
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
- **Key Flexibility**: API keys can be set in `backend/.env` or in the browser's **Gemma AI** settings dialog (saved locally in `localStorage`).
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

## Getting Started

### Prerequisites
- **Rust & Cargo** (v1.75+ or newer)
- **Node.js** (v18+ or newer) and **npm** (or **pnpm**)
- *(Optional)* A Google Gemini API key from [Google AI Studio](https://aistudio.google.com/apikey) for multimodal image/scan transcription and streaming AI chat.

---

### Running the Application

#### Option A: Development Mode (Vite Dev Server + Axum Backend)

1. **Start the Rust Backend**:
   ```bash
   cd backend
   cp .env.example .env    # optional: set GEMINI_API_KEY="your-key"
   cargo run               # runs on http://localhost:3000
   ```

2. **Start the Frontend Dev Server**:
   ```bash
   cd frontend
   npm install             # or pnpm install
   npm run dev             # runs on http://localhost:5173
   ```

#### Option B: Unified Production Build (Single Port `3000`)

The Rust backend is configured to automatically serve the compiled frontend distribution files (`frontend/dist`):

1. **Build the Frontend**:
   ```bash
   cd frontend
   npm install
   npm run build
   ```

2. **Run the Backend**:
   ```bash
   cd backend
   cargo run
   ```

3. Open **`http://localhost:3000`** in your browser. All devices on your local network can access the application via your machine's local IP address (e.g., `http://192.168.x.x:3000`).

---

### Automated Backend Tests

To run the automated Rust unit test suite:
```bash
cd backend
cargo test
```

---

## Deployment

### Deploy on Railway
1. Push your repository to **GitHub**.
2. Create a **New Project** in [Railway](https://railway.com/) and select **Deploy from GitHub repo**.
3. Set the environment variable:
   - `GEMINI_API_KEY`: *(Optional)* Your Gemini API key.
4. Set the build command to compile both frontend and backend:
   ```bash
   cd frontend && npm install && npm run build && cd ../backend && cargo build --release
   ```
5. Set start command:
   ```bash
   ./backend/target/release/invoice-auditor-backend
   ```

### Deploy on Render
1. Connect your repository to [Render](https://render.com/) as a **New Web Service**.
2. **Build Command**:
   ```bash
   cd frontend && npm install && npm run build && cd ../backend && cargo build --release
   ```
3. **Start Command**:
   ```bash
   ./backend/target/release/invoice-auditor-backend
   ```
4. **Environment Variables**:
   - `GEMINI_API_KEY`: *(Optional)* Your Gemini API key.
   - `FRONTEND_DIST`: `frontend/dist`
