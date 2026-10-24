# DocuAudit • Document & Invoice Auditor (with RAG Auto-Summarization)

A local invoice auditor: a **Rust (Axum + Tokio)** backend with audit rules, a **React + TypeScript + Tailwind + shadcn/ui** frontend, an **AI chat assistant**, **scan reading**, and **RAG document auto-summarization** powered by **Gemma 4** (via the Gemini API).

---

## What's New in v0.4 (RAG Auto-Summarization)
- **Automatic Document Indexing**: Every uploaded PDF or JSON invoice is automatically indexed in an in-memory RAG document store.
- **Auto-Summarization**: Instantly generates concise 3-5 bullet point executive summaries of uploaded files (`POST /api/rag/summarize`).
- **Document Store API**: Fetch all indexed documents and their summaries (`GET /api/rag/documents`).

---

## How a file is read & summarized
1. **JSON / TXT / PDF / Scans**: The backend extracts text/images, runs mathematical consistency checks and risk audit rules (`RULE_001` to `RULE_005`), and indexes the content into the RAG store.
2. **Auto-Summary**: Gemma 4 or local heuristics summarize the document immediately upon upload.
3. **Chat Assistant**: Ask questions about any audit finding or document summary in real-time via SSE streaming.

---

## Directory Structure
```
backend/
  Cargo.toml
  .env.example            GEMINI_API_KEY, GEMMA_MODEL, PORT, FRONTEND_DIST
  src/
    main.rs               routes, CORS, static UI, RAG endpoints (/api/rag/summarize)
    rag.rs                RAG document store & auto-summarization logic
    auditor.rs            domain models + 5 audit rules + tests
    parser.rs             strict-first, then lenient JSON parser
    pdf_scanner.rs        native PDF text extraction + heuristic parser
    extract.rs            /api/extract and shared extraction pipeline
    enhance.rs            image upscale / contrast / sharpen
    gemma.rs              Gemma 4 (Gemini API) client
    chat.rs               /api/chat (SSE stream)
frontend/                 React + TS + Tailwind + shadcn/ui
samples/                  clean / faulty / INR / messy JSON + sample_invoice.pdf
```

---

## How to Run It Locally

Prerequisites: Rust 1.75+, Node 18+. A Gemini API key is optional (https://aistudio.google.com/apikey).

```bash
# 1. Backend
cd backend
cp .env.example .env      # optional: paste GEMINI_API_KEY
cargo test
cargo run                 # http://localhost:3000

# 2. Frontend (Dev mode)
cd frontend
npm install
npm run dev               # http://localhost:5173
```

---

## How to Deploy via GitHub

### Option 1: Deploy on Railway (Recommended)
1. Push your repository to **GitHub**.
2. Go to [Railway](https://railway.com/) and click **New Project** -> **Deploy from GitHub repo**.
3. Add your environment variable in settings:
   - `GEMINI_API_KEY` = `your-api-key-here`
4. Generate a public domain. Every `git push` to GitHub will automatically trigger a rebuild and live deployment.

### Option 2: Deploy on Render
1. Connect your GitHub repository to [Render](https://render.com/) as a **New Web Service**.
2. **Build Command**:
   ```bash
   cd frontend && npm install && npm run build && cd ../backend && cargo build --release
   ```
3. **Start Command**:
   ```bash
   ./backend/target/release/invoice-auditor-backend
   ```
4. **Environment Variables**:
   - `GEMINI_API_KEY` = `your-api-key-here`
   - `FRONTEND_DIST` = `frontend/dist`
