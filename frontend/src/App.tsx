import React, { useState, useEffect, useRef } from 'react';
import {
  FileText,
  AlertTriangle,
  CheckCircle2,
  XCircle,
  Copy,
  Check,
  RefreshCw,
  Sparkles,
  Key,
  Upload,
  ArrowRight,
  ShieldAlert,
  Info,
  DollarSign,
  Layers,
  Bot,
  ExternalLink,
  ChevronDown,
  ChevronUp,
  Image as ImageIcon,
} from 'lucide-react';

import {
  AuditReport,
  InvoiceData,
  BackendHealth,
  KeyMode,
} from './types';
import {
  CLEAN_USD_SAMPLE,
  CLEAN_INR_SAMPLE,
  FAULTY_SAMPLE,
  MESSY_JSON_STRING,
} from './samples';
import {
  preprocessInvoiceImage,
  renderScannedPdfPages,
} from './lib/imagePreprocessor';

// SmoothUI Components
import AgentAvatar from './components/smoothui/agent-avatar';
import AIConversation from './components/smoothui/ai-conversation';
import AIMessage from './components/smoothui/ai-message';
import AIPromptInput from './components/smoothui/ai-prompt-input';
import AISuggestions, { AISuggestion } from './components/smoothui/ai-suggestions';
import AIResponse from './components/smoothui/ai-response';
import AILoader from './components/smoothui/ai-loader';
import AIContextMeter from './components/smoothui/ai-context-meter';
import AIApproval, { AIApprovalOption } from './components/smoothui/ai-approval';
import AIArtifact from './components/smoothui/ai-artifact';
import AICitation from './components/smoothui/ai-citation';
import AISources, { AISource } from './components/smoothui/ai-sources';

// Resolve Backend API Base
const API_BASE =
  window.location.port === '5173'
    ? `http://${window.location.hostname}:3000`
    : '';

interface ChatMsg {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  isStreaming?: boolean;
  approval?: {
    question: string;
    options: AIApprovalOption[];
    resolvedId?: string;
  };
  artifact?: {
    title: string;
    code: string;
  };
}

const DEFAULT_SUGGESTIONS: AISuggestion[] = [
  { id: '1', label: 'Why did this invoice fail?' },
  { id: '2', label: 'Break down subtotal & line-item math' },
  { id: '3', label: 'Draft email to vendor requesting correction' },
  { id: '4', label: 'Is the vendor suspicious or placeholder?' },
  { id: '5', label: 'Explain the INR currency audit threshold' },
];

export default function App() {
  // Application State
  const [report, setReport] = useState<AuditReport | null>(null);
  const [jsonText, setJsonText] = useState<string>('');
  const [isLoading, setIsLoading] = useState<boolean>(false);
  const [loadingStatus, setLoadingStatus] = useState<string>('');
  const [copied, setCopied] = useState<boolean>(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  // Backend Health
  const [backendHealth, setBackendHealth] = useState<BackendHealth | null>(null);
  const [isHealthChecking, setIsHealthChecking] = useState<boolean>(false);

  // Key Mode State
  const [browserApiKey, setBrowserApiKey] = useState<string>(() => {
    return localStorage.getItem('docuaudit_gemini_key') || '';
  });
  const [isAiModalOpen, setIsAiModalOpen] = useState<boolean>(false);
  const [apiKeyInput, setApiKeyInput] = useState<string>('');

  // Chat State
  const [isChatOpen, setIsChatOpen] = useState<boolean>(false);
  const [chatMessages, setChatMessages] = useState<ChatMsg[]>([
    {
      id: 'init',
      role: 'assistant',
      content:
        'Hello! I am DocuAudit AI powered by Gemma. I have real-time context of your currently audited invoice and report findings. Feel free to ask about math errors, flagged risks, or request draft vendor emails.',
    },
  ]);
  const [isAiResponding, setIsAiResponding] = useState<boolean>(false);
  const [chatStreamAbort, setChatStreamAbort] = useState<AbortController | null>(null);

  // Page thumbnails if multi-page scanned
  const [pageThumbnails, setPageThumbnails] = useState<string[]>([]);
  const [showDerivedFields, setShowDerivedFields] = useState<boolean>(true);

  // File input ref
  const fileInputRef = useRef<HTMLInputElement>(null);

  // Derive active key mode
  const keyMode: KeyMode = backendHealth?.has_server_key
    ? 'SERVER'
    : browserApiKey.trim().length > 0
    ? 'BROWSER'
    : 'NATIVE';

  // Check health on mount and every 15 seconds
  const fetchHealth = async () => {
    setIsHealthChecking(true);
    try {
      const res = await fetch(`${API_BASE}/health`);
      if (res.ok) {
        const data: BackendHealth = await res.json();
        setBackendHealth(data);
      } else {
        setBackendHealth(null);
      }
    } catch {
      setBackendHealth(null);
    } finally {
      setIsHealthChecking(false);
    }
  };

  useEffect(() => {
    fetchHealth();
    const interval = setInterval(fetchHealth, 15000);
    return () => clearInterval(interval);
  }, []);

  // Initialize with Clean Sample on start
  useEffect(() => {
    handleAuditJson(JSON.stringify(CLEAN_USD_SAMPLE, null, 2));
  }, []);

  // Helper to get request headers with optional browser API key
  const getHeaders = (extra: Record<string, string> = {}): Record<string, string> => {
    const headers: Record<string, string> = { ...extra };
    if (browserApiKey.trim()) {
      headers['x-gemini-api-key'] = browserApiKey.trim();
    }
    return headers;
  };

  // Direct JSON audit
  const handleAuditJson = async (rawJson: string) => {
    setIsLoading(true);
    setErrorMsg(null);
    setLoadingStatus('Validating and auditing invoice payload...');
    setPageThumbnails([]);
    try {
      const res = await fetch(`${API_BASE}/api/audit-json`, {
        method: 'POST',
        headers: getHeaders({ 'Content-Type': 'application/json' }),
        body: rawJson,
      });

      if (!res.ok) {
        const err = await res.json();
        throw new Error(err.error || 'Failed to audit JSON invoice');
      }

      const auditReport: AuditReport = await res.json();
      setReport(auditReport);
      setJsonText(JSON.stringify(auditReport.parsed_invoice, null, 2));
    } catch (err: any) {
      setErrorMsg(err.message || 'An error occurred during JSON auditing');
    } finally {
      setIsLoading(false);
      setLoadingStatus('');
    }
  };

  // Upload handler for PDFs and files
  const handleFileUpload = async (files: FileList | File[]) => {
    if (!files || files.length === 0) return;
    setIsLoading(true);
    setErrorMsg(null);
    setPageThumbnails([]);

    try {
      const firstFile = files[0];
      const isPdf = firstFile.name.toLowerCase().endsWith('.pdf') || firstFile.type === 'application/pdf';
      const isImage = firstFile.type.startsWith('image/') || /\.(png|jpe?g|webp)$/i.test(firstFile.name);

      // Scenario 1: Multi-page images or single photo/scan (up to 8 images)
      if (isImage || (files.length > 1 && !isPdf)) {
        const imageFiles = Array.from(files).slice(0, 8);
        setLoadingStatus(
          `Upscaling 4x, contrast-stretching, and sharpening ${imageFiles.length} page image(s)...`
        );

        const processedImages = [];
        const thumbs = [];
        for (let i = 0; i < imageFiles.length; i++) {
          const preprocessed = await preprocessInvoiceImage(imageFiles[i]);
          processedImages.push({
            mime_type: preprocessed.mimeType,
            data: preprocessed.data,
          });
          thumbs.push(preprocessed.dataUrl);
        }
        setPageThumbnails(thumbs);

        setLoadingStatus(
          `Transcribing ${processedImages.length} page(s) verbatim with Gemma Vision OCR...`
        );

        const res = await fetch(`${API_BASE}/api/extract`, {
          method: 'POST',
          headers: getHeaders({ 'Content-Type': 'application/json' }),
          body: JSON.stringify({ images: processedImages }),
        });

        if (!res.ok) {
          const err = await res.json();
          throw new Error(err.error || 'Gemma image transcription failed');
        }

        const auditReport: AuditReport = await res.json();
        setReport(auditReport);
        setJsonText(JSON.stringify(auditReport.parsed_invoice, null, 2));
        return;
      }

      // Scenario 2: PDF File
      if (isPdf) {
        setLoadingStatus('Reading digital PDF text layer...');
        const formData = new FormData();
        formData.append('file', firstFile);
        if (browserApiKey.trim()) {
          formData.append('api_key', browserApiKey.trim());
        }

        const res = await fetch(`${API_BASE}/api/upload`, {
          method: 'POST',
          headers: getHeaders(),
          body: formData,
        });

        if (res.ok) {
          const auditReport: AuditReport = await res.json();
          setReport(auditReport);
          setJsonText(JSON.stringify(auditReport.parsed_invoice, null, 2));
          return;
        }

        // If digital extraction failed and it's a scanned PDF, render in browser
        setLoadingStatus('Rendering scanned PDF pages in browser (up to 5 pages)...');
        const buffer = await firstFile.arrayBuffer();
        const pdfImages = await renderScannedPdfPages(new Uint8Array(buffer), 5);
        if (pdfImages.length === 0) {
          throw new Error('Could not render pages from scanned PDF');
        }

        setPageThumbnails(pdfImages.map((p) => p.dataUrl));
        setLoadingStatus(
          `Gemma Vision OCR transcribing ${pdfImages.length} scanned PDF page(s)...`
        );

        const extractRes = await fetch(`${API_BASE}/api/extract`, {
          method: 'POST',
          headers: getHeaders({ 'Content-Type': 'application/json' }),
          body: JSON.stringify({
            images: pdfImages.map((p) => ({
              mime_type: p.mimeType,
              data: p.data,
            })),
          }),
        });

        if (!extractRes.ok) {
          const err = await extractRes.json();
          throw new Error(err.error || 'Failed scanning PDF with Gemma');
        }

        const auditReport: AuditReport = await extractRes.json();
        setReport(auditReport);
        setJsonText(JSON.stringify(auditReport.parsed_invoice, null, 2));
        return;
      }

      // Scenario 3: Text / JSON file upload
      setLoadingStatus('Parsing text/JSON invoice...');
      const formData = new FormData();
      formData.append('file', firstFile);
      const res = await fetch(`${API_BASE}/api/upload`, {
        method: 'POST',
        headers: getHeaders(),
        body: formData,
      });

      if (!res.ok) {
        const err = await res.json();
        throw new Error(err.error || 'Failed processing uploaded file');
      }

      const auditReport: AuditReport = await res.json();
      setReport(auditReport);
      setJsonText(JSON.stringify(auditReport.parsed_invoice, null, 2));
    } catch (err: any) {
      setErrorMsg(err.message || 'Failed to process document');
    } finally {
      setIsLoading(false);
      setLoadingStatus('');
    }
  };

  // Streaming Chat with Gemma Assistant
  const handleSendMessage = async (userPrompt: string) => {
    if (!userPrompt.trim()) return;

    const userMsgId = `user-${Date.now()}`;
    const botMsgId = `bot-${Date.now()}`;

    const newMessages: ChatMsg[] = [
      ...chatMessages,
      { id: userMsgId, role: 'user', content: userPrompt },
      { id: botMsgId, role: 'assistant', content: '', isStreaming: true },
    ];
    setChatMessages(newMessages);
    setIsAiResponding(true);

    const abortCtrl = new AbortController();
    setChatStreamAbort(abortCtrl);

    try {
      const chatPayload = {
        messages: newMessages
          .filter((m) => m.id !== botMsgId)
          .map((m) => ({ role: m.role, content: m.content })),
        audit_report: report,
        invoice: report?.parsed_invoice,
      };

      const res = await fetch(`${API_BASE}/api/chat`, {
        method: 'POST',
        headers: getHeaders({ 'Content-Type': 'application/json' }),
        body: JSON.stringify(chatPayload),
        signal: abortCtrl.signal,
      });

      if (!res.ok || !res.body) {
        throw new Error('Streaming connection failed');
      }

      const reader = res.body.getReader();
      const decoder = new TextDecoder();
      let accumulated = '';

      while (true) {
        const { value, done } = await reader.read();
        if (done) break;

        const chunk = decoder.decode(value, { stream: true });
        const lines = chunk.split('\n');

        for (const line of lines) {
          const trimmed = line.trim();
          if (trimmed.startsWith('data: ')) {
            const dataStr = trimmed.slice(6);
            if (dataStr === '[DONE]') {
              break;
            }
            try {
              const parsed = JSON.parse(dataStr);
              if (parsed.text) {
                accumulated += parsed.text;
                setChatMessages((prev) =>
                  prev.map((m) =>
                    m.id === botMsgId
                      ? { ...m, content: accumulated, isStreaming: true }
                      : m
                  )
                );
              }
            } catch {
              // Ignore non-json SSE lines
            }
          }
        }
      }

      // Finalize message and add interactive approval / artifact if context matches
      setChatMessages((prev) =>
        prev.map((m) => {
          if (m.id === botMsgId) {
            let approval;
            let artifact;

            if (userPrompt.toLowerCase().includes('email') || accumulated.toLowerCase().includes('subject:')) {
              artifact = {
                title: 'Vendor Dispute Email Draft.txt',
                code: accumulated,
              };
            } else if (report && report.high_risk_count > 0 && userPrompt.toLowerCase().includes('fail')) {
              approval = {
                question: 'Dual-Authorization Action: Flag vendor for CFO compliance audit?',
                options: [
                  {
                    id: 'flag',
                    label: 'Flag Vendor for Formal Audit',
                    detail: 'Hold payment and request vendor re-submission',
                    destructive: true,
                  },
                  {
                    id: 'allow',
                    label: 'Bypass with CFO Sign-off',
                    detail: 'Authorize manual accounting override',
                  },
                ],
              };
            }

            return {
              ...m,
              content: accumulated,
              isStreaming: false,
              approval,
              artifact,
            };
          }
          return m;
        })
      );
    } catch (err: any) {
      if (err.name !== 'AbortError') {
        setChatMessages((prev) =>
          prev.map((m) =>
            m.id === botMsgId
              ? {
                  ...m,
                  content: `⚠️ Error communicating with Gemma AI: ${err.message}`,
                  isStreaming: false,
                }
              : m
          )
        );
      }
    } finally {
      setIsAiResponding(false);
      setChatStreamAbort(null);
    }
  };

  const handleStopGeneration = () => {
    if (chatStreamAbort) {
      chatStreamAbort.abort();
      setChatStreamAbort(null);
      setIsAiResponding(false);
    }
  };

  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const saveApiKey = () => {
    const cleaned = apiKeyInput.trim();
    setBrowserApiKey(cleaned);
    localStorage.setItem('docuaudit_gemini_key', cleaned);
    setIsAiModalOpen(false);
  };

  const clearApiKey = () => {
    setBrowserApiKey('');
    setApiKeyInput('');
    localStorage.removeItem('docuaudit_gemini_key');
    setIsAiModalOpen(false);
  };

  // Sources for AISources component
  const auditSources: AISource[] = [
    {
      id: 'doc',
      title: report ? `Invoice ${report.invoice_id}` : 'Invoice Document',
      snippet: report
        ? `Vendor: ${report.vendor_name} • Total: ${report.parsed_invoice.currency} ${report.parsed_invoice.total_amount.toFixed(2)}`
        : 'Uploaded invoice document data',
      url: 'https://docuaudit.local/document',
    },
    {
      id: 'policy',
      title: 'Corporate Financial Audit Guidelines (2026)',
      snippet: 'Rules: Empty items, subtotal math, tax discrepancy, USD/INR dual-authorization, vendor credibility.',
      url: 'https://docuaudit.local/policy',
    },
  ];

  return (
    <div className="min-h-screen bg-[#090a0f] text-zinc-100 flex flex-col font-sans selection:bg-blue-500/30 selection:text-blue-200">
      {/* Top Navigation */}
      <header className="border-b border-zinc-800/80 bg-[#0c0d14]/90 backdrop-blur sticky top-0 z-30 px-4 lg:px-8 py-3.5 flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-xl bg-gradient-to-br from-blue-600 via-indigo-600 to-violet-600 flex items-center justify-center shadow-lg shadow-blue-500/20 ring-1 ring-white/20">
            <ShieldAlert className="w-5 h-5 text-white" />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <h1 className="text-lg font-bold tracking-tight text-white flex items-center gap-1.5">
                DocuAudit
                <span className="text-[10px] font-semibold uppercase px-2 py-0.5 rounded-full bg-blue-500/10 text-blue-400 border border-blue-500/20">
                  v0.2
                </span>
              </h1>
            </div>
            <p className="text-xs text-zinc-400 font-medium">
              Automated Document & Invoice Integrity Auditor
            </p>
          </div>
        </div>

        <div className="flex items-center gap-2.5">
          {/* Key Mode Status Badge */}
          <button
            onClick={() => {
              setApiKeyInput(browserApiKey);
              setIsAiModalOpen(true);
            }}
            className="flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs font-medium border transition-all bg-zinc-900/80 hover:bg-zinc-800 border-zinc-700/60"
            title="Configure Gemma AI Key"
          >
            <Sparkles className="w-3.5 h-3.5 text-blue-400" />
            <span className="hidden sm:inline text-zinc-300">Gemma AI</span>
            <span
              className={`px-2 py-0.5 rounded-full text-[11px] font-semibold border ${
                keyMode === 'SERVER'
                  ? 'bg-emerald-500/15 text-emerald-300 border-emerald-500/30'
                  : keyMode === 'BROWSER'
                  ? 'bg-purple-500/15 text-purple-300 border-purple-500/30'
                  : 'bg-zinc-700/30 text-zinc-400 border-zinc-700/50'
              }`}
            >
              {keyMode === 'SERVER'
                ? 'Server key'
                : keyMode === 'BROWSER'
                ? 'Browser key'
                : 'Native mode'}
            </span>
          </button>

          {/* Backend Status Indicator */}
          <div className="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-zinc-900/80 border border-zinc-800 text-xs">
            <span
              className={`w-2 h-2 rounded-full ${
                backendHealth?.status === 'ok'
                  ? 'bg-emerald-400 shadow-[0_0_8px_rgba(52,211,153,0.6)] animate-pulse'
                  : 'bg-rose-500 shadow-[0_0_8px_rgba(244,63,94,0.6)]'
              }`}
            />
            <span className="hidden md:inline text-zinc-400 font-mono">
              {backendHealth?.status === 'ok' ? 'Backend Live' : 'Backend Disconnected'}
            </span>
            <button
              onClick={fetchHealth}
              disabled={isHealthChecking}
              className="text-zinc-500 hover:text-zinc-300 p-0.5 rounded transition-transform active:rotate-180"
              title="Refresh backend status"
            >
              <RefreshCw className={`w-3.5 h-3.5 ${isHealthChecking ? 'animate-spin' : ''}`} />
            </button>
          </div>

          {/* Chat Assistant Toggle Button */}
          <button
            onClick={() => setIsChatOpen(!isChatOpen)}
            className={`flex items-center gap-2 px-3.5 py-1.5 rounded-lg text-xs font-semibold shadow-md transition-all ${
              isChatOpen
                ? 'bg-blue-600 text-white shadow-blue-500/25 ring-2 ring-blue-400/40'
                : 'bg-zinc-800 hover:bg-zinc-700 text-zinc-200 border border-zinc-700'
            }`}
          >
            <Bot className="w-4 h-4 text-blue-300" />
            <span className="hidden sm:inline">Audit Assistant</span>
          </button>
        </div>
      </header>

      {/* Main Container */}
      <div className="flex-1 flex overflow-hidden">
        <main className="flex-1 overflow-y-auto p-4 lg:p-6 space-y-6 max-w-7xl mx-auto w-full">
          {/* Action & Dropzone Bar */}
          <section className="bg-zinc-900/60 border border-zinc-800/80 rounded-2xl p-5 shadow-xl backdrop-blur-sm">
            <div
              onDragOver={(e) => e.preventDefault()}
              onDrop={(e) => {
                e.preventDefault();
                if (e.dataTransfer.files) {
                  handleFileUpload(e.dataTransfer.files);
                }
              }}
              onClick={() => fileInputRef.current?.click()}
              className="border-2 border-dashed border-zinc-700/70 hover:border-blue-500/70 rounded-xl p-6 text-center cursor-pointer transition-all bg-zinc-950/40 hover:bg-blue-950/10 group"
            >
              <input
                ref={fileInputRef}
                type="file"
                multiple
                accept=".json,.txt,.pdf,.png,.jpg,.jpeg,.webp,application/pdf"
                className="hidden"
                onChange={(e) => {
                  if (e.target.files) {
                    handleFileUpload(e.target.files);
                  }
                }}
              />
              <div className="w-12 h-12 mx-auto mb-3 rounded-xl bg-zinc-800/80 group-hover:bg-blue-600/20 text-zinc-400 group-hover:text-blue-400 flex items-center justify-center transition-all ring-1 ring-zinc-700 group-hover:ring-blue-500/30">
                <Upload className="w-6 h-6" />
              </div>
              <p className="text-sm font-semibold text-zinc-200 group-hover:text-white">
                Drag & drop invoices, PDFs, photos, or{' '}
                <span className="text-blue-400 underline decoration-blue-500/40 underline-offset-4">
                  browse files
                </span>
              </p>
              <p className="text-xs text-zinc-400 mt-1.5 max-w-lg mx-auto">
                Supports <code>.json</code> (auto-normalized), digital & scanned <code>.pdf</code>, and photos (
                <code>PNG</code>, <code>JPG</code>, <code>WEBP</code> up to 8 pages with 4x upscale & sharpening)
              </p>
            </div>

            {/* Quick Test Scenarios */}
            <div className="mt-4 pt-4 border-t border-zinc-800/60 flex flex-wrap items-center justify-between gap-3">
              <span className="text-xs font-semibold uppercase tracking-wider text-zinc-400">
                Instant Test Scenarios:
              </span>
              <div className="flex flex-wrap gap-2">
                <button
                  onClick={() => handleAuditJson(JSON.stringify(CLEAN_USD_SAMPLE, null, 2))}
                  className="px-3 py-1.5 rounded-lg bg-zinc-800/80 hover:bg-zinc-700 border border-zinc-700/80 text-xs font-medium text-zinc-200 flex items-center gap-1.5 transition-colors"
                >
                  <span>⚡</span> Clean (USD)
                </button>
                <button
                  onClick={() => handleAuditJson(JSON.stringify(CLEAN_INR_SAMPLE, null, 2))}
                  className="px-3 py-1.5 rounded-lg bg-zinc-800/80 hover:bg-zinc-700 border border-zinc-700/80 text-xs font-medium text-zinc-200 flex items-center gap-1.5 transition-colors"
                >
                  <span>🇮🇳</span> Clean (INR ₹)
                </button>
                <button
                  onClick={() => handleAuditJson(MESSY_JSON_STRING)}
                  className="px-3 py-1.5 rounded-lg bg-zinc-800/80 hover:bg-zinc-700 border border-zinc-700/80 text-xs font-medium text-amber-300 flex items-center gap-1.5 transition-colors"
                  title="Tests camelCase, wrapped objects, string numbers, and currency symbols mapped without AI"
                >
                  <span>🌀</span> Messy JSON
                </button>
                <button
                  onClick={() => handleAuditJson(JSON.stringify(FAULTY_SAMPLE, null, 2))}
                  className="px-3 py-1.5 rounded-lg bg-zinc-800/80 hover:bg-zinc-700 border border-rose-500/30 text-xs font-medium text-rose-300 flex items-center gap-1.5 transition-colors"
                >
                  <span>⚠️</span> Faulty Sample
                </button>
              </div>
            </div>
          </section>

          {/* Loading Banner */}
          {isLoading && (
            <div className="bg-blue-950/40 border border-blue-500/40 rounded-xl p-4 flex items-center gap-3 animate-pulse">
              <AILoader variant="bar" />
              <div>
                <p className="text-sm font-semibold text-blue-200">{loadingStatus}</p>
                <p className="text-xs text-blue-400">Analyzing document arithmetic and auditing rules...</p>
              </div>
            </div>
          )}

          {/* Error Banner */}
          {errorMsg && (
            <div className="bg-rose-950/40 border border-rose-500/40 rounded-xl p-4 flex items-start gap-3">
              <XCircle className="w-5 h-5 text-rose-400 shrink-0 mt-0.5" />
              <div className="flex-1 text-sm text-rose-200">
                <p className="font-semibold">Document Processing Error</p>
                <p className="text-xs text-rose-300 mt-0.5">{errorMsg}</p>
              </div>
              <button
                onClick={() => setErrorMsg(null)}
                className="text-rose-400 hover:text-rose-200 text-xs font-bold px-2 py-1"
              >
                Dismiss
              </button>
            </div>
          )}

          {/* Scanned Multi-Page Thumbnails Preview */}
          {pageThumbnails.length > 0 && (
            <div className="bg-zinc-900/60 border border-zinc-800 rounded-xl p-4">
              <div className="flex items-center gap-2 mb-3">
                <ImageIcon className="w-4 h-4 text-blue-400" />
                <h4 className="text-xs font-bold uppercase tracking-wider text-zinc-300">
                  Preprocessed Page Images ({pageThumbnails.length} Pages • Contrast-Stretched & Sharpened)
                </h4>
              </div>
              <div className="grid grid-cols-2 sm:grid-cols-4 md:grid-cols-6 gap-3">
                {pageThumbnails.map((thumb, idx) => (
                  <div
                    key={idx}
                    className="relative group rounded-lg overflow-hidden border border-zinc-700 bg-black aspect-[3/4]"
                  >
                    <img
                      src={thumb}
                      alt={`Page ${idx + 1}`}
                      className="w-full h-full object-cover group-hover:scale-105 transition-transform"
                    />
                    <span className="absolute bottom-1 right-1 px-1.5 py-0.5 rounded bg-black/80 text-[10px] font-mono text-zinc-300">
                      Page {idx + 1}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* Main Comparison Layout */}
          {report && (
            <div className="grid grid-cols-1 lg:grid-cols-12 gap-6 items-start">
              {/* Left Column: Extraction Details & JSON Viewer */}
              <div className="lg:col-span-6 space-y-4">
                {/* Extraction Metadata Card */}
                {report.extraction && (
                  <div className="bg-zinc-900/70 border border-zinc-800 rounded-xl p-4 space-y-3">
                    <div className="flex items-center justify-between">
                      <div className="flex items-center gap-2">
                        <Layers className="w-4 h-4 text-blue-400" />
                        <span className="text-xs font-bold uppercase tracking-wider text-zinc-300">
                          Extraction Origin
                        </span>
                      </div>
                      <span
                        className={`text-[11px] font-bold px-2 py-0.5 rounded-full border ${
                          report.extraction.confidence === 'High'
                            ? 'bg-emerald-500/10 text-emerald-300 border-emerald-500/20'
                            : 'bg-amber-500/10 text-amber-300 border-amber-500/20'
                        }`}
                      >
                        {report.extraction.confidence} Confidence
                      </span>
                    </div>

                    <div className="grid grid-cols-2 gap-3 text-xs">
                      <div className="bg-zinc-950/60 p-2.5 rounded-lg border border-zinc-800/80">
                        <span className="text-zinc-500 block text-[10px] uppercase font-semibold">
                          Method
                        </span>
                        <span className="font-semibold text-zinc-200">{report.extraction.method}</span>
                      </div>
                      <div className="bg-zinc-950/60 p-2.5 rounded-lg border border-zinc-800/80">
                        <span className="text-zinc-500 block text-[10px] uppercase font-semibold">
                          Model / Engine
                        </span>
                        <span className="font-semibold text-zinc-200">{report.extraction.model}</span>
                      </div>
                    </div>

                    <p className="text-xs text-zinc-400 bg-zinc-950/30 p-2 rounded border border-zinc-800/50">
                      <span className="font-semibold text-zinc-300">Notes:</span> {report.extraction.notes}
                    </p>

                    {/* Derived & Defaulted Fields List (Transparency Rule) */}
                    {report.extraction.derived_fields && report.extraction.derived_fields.length > 0 ? (
                      <div className="border border-amber-500/25 bg-amber-500/5 rounded-lg p-3">
                        <button
                          onClick={() => setShowDerivedFields(!showDerivedFields)}
                          className="w-full flex items-center justify-between text-left text-xs font-semibold text-amber-300"
                        >
                          <span className="flex items-center gap-1.5">
                            <Info className="w-3.5 h-3.5" />
                            Documented Normalizations ({report.extraction.derived_fields.length} Fields)
                          </span>
                          {showDerivedFields ? <ChevronUp className="w-3.5 h-3.5" /> : <ChevronDown className="w-3.5 h-3.5" />}
                        </button>
                        {showDerivedFields && (
                          <div className="mt-2 space-y-1.5">
                            <p className="text-[11px] text-amber-200/70">
                              Every defaulted, mapped, or derived field is explicitly audited:
                            </p>
                            <ul className="space-y-1">
                              {report.extraction.derived_fields.map((field, i) => (
                                <li
                                  key={i}
                                  className="text-[11px] font-mono text-zinc-300 bg-zinc-900/80 px-2 py-1 rounded border border-zinc-800 flex items-start gap-1.5"
                                >
                                  <span className="text-amber-400 font-bold">•</span>
                                  <span>{field}</span>
                                </li>
                              ))}
                            </ul>
                          </div>
                        )}
                      </div>
                    ) : (
                      <div className="text-[11px] text-emerald-400 bg-emerald-950/20 px-3 py-1.5 rounded border border-emerald-900/30 flex items-center gap-1.5">
                        <CheckCircle2 className="w-3.5 h-3.5" />
                        Schema-valid JSON parsed untouched without any defaulted or derived fields.
                      </div>
                    )}
                  </div>
                )}

                {/* JSON Data Viewer */}
                <div className="bg-zinc-900/70 border border-zinc-800 rounded-xl overflow-hidden shadow-lg">
                  <div className="bg-zinc-950/80 px-4 py-2.5 border-b border-zinc-800 flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <FileText className="w-4 h-4 text-zinc-400" />
                      <span className="text-xs font-bold uppercase tracking-wider text-zinc-300 font-mono">
                        Parsed Invoice Payload
                      </span>
                    </div>
                    <button
                      onClick={() => copyToClipboard(jsonText)}
                      className="flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs font-medium bg-zinc-800 hover:bg-zinc-700 text-zinc-200 border border-zinc-700 transition-colors"
                      title="Copy JSON to clipboard"
                    >
                      {copied ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
                      <span>{copied ? 'Copied' : 'Copy'}</span>
                    </button>
                  </div>
                  <pre className="p-4 text-xs font-mono text-blue-200/90 overflow-x-auto max-h-[480px] bg-[#07080c] leading-relaxed">
                    {jsonText}
                  </pre>
                </div>
              </div>

              {/* Right Column: Findings Panel & Risk Reports */}
              <div className="lg:col-span-6 space-y-4">
                {/* Overall Status Banner */}
                <div
                  className={`rounded-xl p-5 border shadow-xl flex items-center justify-between ${
                    report.overall_status === 'PASSED'
                      ? 'bg-emerald-950/30 border-emerald-500/40 text-emerald-200'
                      : report.overall_status === 'REVIEW_RECOMMENDED'
                      ? 'bg-amber-950/30 border-amber-500/40 text-amber-200'
                      : 'bg-rose-950/30 border-rose-500/40 text-rose-200'
                  }`}
                >
                  <div className="flex items-center gap-3.5">
                    <div
                      className={`w-12 h-12 rounded-xl flex items-center justify-center shrink-0 ${
                        report.overall_status === 'PASSED'
                          ? 'bg-emerald-500/20 text-emerald-400'
                          : report.overall_status === 'REVIEW_RECOMMENDED'
                          ? 'bg-amber-500/20 text-amber-400'
                          : 'bg-rose-500/20 text-rose-400'
                      }`}
                    >
                      {report.overall_status === 'PASSED' ? (
                        <CheckCircle2 className="w-7 h-7" />
                      ) : report.overall_status === 'REVIEW_RECOMMENDED' ? (
                        <AlertTriangle className="w-7 h-7" />
                      ) : (
                        <XCircle className="w-7 h-7" />
                      )}
                    </div>
                    <div>
                      <div className="flex items-center gap-2">
                        <span className="text-xs font-semibold uppercase tracking-wider opacity-80">
                          Audit Verdict
                        </span>
                      </div>
                      <h3 className="text-xl font-bold tracking-tight text-white">
                        {report.overall_status === 'PASSED'
                          ? 'Passed All Checks'
                          : report.overall_status === 'REVIEW_RECOMMENDED'
                          ? 'Review Recommended'
                          : 'Action Required'}
                      </h3>
                      <p className="text-xs mt-0.5 opacity-80">
                        {report.total_flags === 0
                          ? 'Zero arithmetic or vendor discrepancies identified.'
                          : `${report.total_flags} verification flag(s) identified for auditor attention.`}
                      </p>
                    </div>
                  </div>

                  <div className="text-right font-mono">
                    <span className="text-xs text-zinc-400 block font-sans">Currency</span>
                    <span className="text-lg font-bold text-white">
                      {report.parsed_invoice.currency === 'INR' ? '₹ INR' : '$ USD'}
                    </span>
                  </div>
                </div>

                {/* Risk Counter Metrics Grid */}
                <div className="grid grid-cols-4 gap-3">
                  <div className="bg-zinc-900/60 border border-zinc-800 rounded-xl p-3 text-center">
                    <span className="text-[10px] uppercase font-bold text-zinc-500 block tracking-wider">
                      High Risk
                    </span>
                    <span
                      className={`text-2xl font-black font-mono mt-0.5 block ${
                        report.high_risk_count > 0 ? 'text-rose-400' : 'text-zinc-500'
                      }`}
                    >
                      {report.high_risk_count}
                    </span>
                  </div>
                  <div className="bg-zinc-900/60 border border-zinc-800 rounded-xl p-3 text-center">
                    <span className="text-[10px] uppercase font-bold text-zinc-500 block tracking-wider">
                      Med Risk
                    </span>
                    <span
                      className={`text-2xl font-black font-mono mt-0.5 block ${
                        report.medium_risk_count > 0 ? 'text-amber-400' : 'text-zinc-500'
                      }`}
                    >
                      {report.medium_risk_count}
                    </span>
                  </div>
                  <div className="bg-zinc-900/60 border border-zinc-800 rounded-xl p-3 text-center">
                    <span className="text-[10px] uppercase font-bold text-zinc-500 block tracking-wider">
                      Low Risk
                    </span>
                    <span className="text-2xl font-black font-mono mt-0.5 block text-blue-400">
                      {report.low_risk_count}
                    </span>
                  </div>
                  <div className="bg-zinc-900/60 border border-zinc-800 rounded-xl p-3 text-center">
                    <span className="text-[10px] uppercase font-bold text-zinc-500 block tracking-wider">
                      Total Flags
                    </span>
                    <span className="text-2xl font-black font-mono mt-0.5 block text-white">
                      {report.total_flags}
                    </span>
                  </div>
                </div>

                {/* Invoice Summary Strip */}
                <div className="bg-zinc-900/50 border border-zinc-800/80 rounded-xl p-3.5 grid grid-cols-2 sm:grid-cols-4 gap-2 text-xs">
                  <div>
                    <span className="text-zinc-500 block text-[10px] uppercase">Vendor</span>
                    <span className="font-semibold text-zinc-200 truncate block">
                      {report.parsed_invoice.vendor_name || 'Missing Vendor'}
                    </span>
                  </div>
                  <div>
                    <span className="text-zinc-500 block text-[10px] uppercase">Invoice ID</span>
                    <span className="font-mono font-semibold text-zinc-200 truncate block">
                      {report.parsed_invoice.invoice_id}
                    </span>
                  </div>
                  <div>
                    <span className="text-zinc-500 block text-[10px] uppercase">Line Items</span>
                    <span className="font-semibold text-zinc-200 block">
                      {report.parsed_invoice.items.length} items
                    </span>
                  </div>
                  <div>
                    <span className="text-zinc-500 block text-[10px] uppercase">Total Amount</span>
                    <span className="font-mono font-bold text-blue-400 block">
                      {report.parsed_invoice.currency === 'INR' ? '₹' : '$'}{' '}
                      {report.parsed_invoice.total_amount.toLocaleString(undefined, {
                        minimumFractionDigits: 2,
                        maximumFractionDigits: 2,
                      })}
                    </span>
                  </div>
                </div>

                {/* Itemized Findings Cards */}
                <div className="space-y-3">
                  <h4 className="text-xs font-bold uppercase tracking-wider text-zinc-400 flex items-center justify-between">
                    <span>Itemized Audit Findings ({report.flags.length})</span>
                    <span className="text-[11px] text-zinc-500 normal-case font-normal">
                      5 Audit Rules Evaluated
                    </span>
                  </h4>

                  {report.flags.length === 0 ? (
                    <div className="bg-emerald-950/15 border border-emerald-500/20 rounded-xl p-8 text-center">
                      <CheckCircle2 className="w-10 h-10 text-emerald-400 mx-auto mb-2 opacity-90" />
                      <p className="text-sm font-bold text-emerald-200">Zero Audit Flags</p>
                      <p className="text-xs text-zinc-400 max-w-sm mx-auto mt-1">
                        All line item arithmetic, subtotals, tax equations, currency thresholds, and vendor credibility checks passed.
                      </p>
                    </div>
                  ) : (
                    report.flags.map((flag, idx) => (
                      <div
                        key={idx}
                        className={`rounded-xl p-4 border transition-all ${
                          flag.risk_level === 'HIGH'
                            ? 'bg-rose-950/20 border-rose-500/40 hover:border-rose-500/60'
                            : flag.risk_level === 'MEDIUM'
                            ? 'bg-amber-950/20 border-amber-500/40 hover:border-amber-500/60'
                            : 'bg-blue-950/20 border-blue-500/40 hover:border-blue-500/60'
                        }`}
                      >
                        <div className="flex items-start justify-between gap-3">
                          <div className="space-y-1">
                            <div className="flex items-center gap-2">
                              <span
                                className={`text-[10px] font-black uppercase px-2 py-0.5 rounded-full border ${
                                  flag.risk_level === 'HIGH'
                                    ? 'bg-rose-500/20 text-rose-300 border-rose-500/30'
                                    : flag.risk_level === 'MEDIUM'
                                    ? 'bg-amber-500/20 text-amber-300 border-amber-500/30'
                                    : 'bg-blue-500/20 text-blue-300 border-blue-500/30'
                                }`}
                              >
                                {flag.risk_level} RISK
                              </span>
                              <span className="text-[11px] font-mono text-zinc-500">{flag.rule_id}</span>
                            </div>
                            <h5 className="text-sm font-bold text-white">{flag.title}</h5>
                            <p className="text-xs text-zinc-300 leading-relaxed">{flag.description}</p>
                          </div>
                        </div>
                      </div>
                    ))
                  )}
                </div>
              </div>
            </div>
          )}
        </main>

        {/* Streaming Chat Assistant Drawer (Powered by SmoothUI) */}
        {isChatOpen && (
          <aside className="w-full lg:w-[460px] border-l border-zinc-800 bg-[#0c0d14] flex flex-col shadow-2xl z-20">
            {/* Chat Header */}
            <div className="p-4 border-b border-zinc-800 flex items-center justify-between bg-zinc-950/70">
              <div className="flex items-center gap-3">
                <AgentAvatar seed="DocuAudit-Gemma" size={34} animated={true} />
                <div>
                  <h3 className="text-sm font-bold text-white flex items-center gap-1.5">
                    DocuAudit Assistant
                  </h3>
                  <p className="text-[11px] text-zinc-400">Streaming Gemma • Full Audit Context</p>
                </div>
              </div>

              <div className="flex items-center gap-2">
                {/* Context Meter */}
                <AIContextMeter
                  limit={128000}
                  used={report ? 4200 : 1200}
                  className="scale-90"
                />
                <button
                  onClick={() => setIsChatOpen(false)}
                  className="text-zinc-500 hover:text-zinc-300 p-1 rounded-md"
                >
                  <XCircle className="w-5 h-5" />
                </button>
              </div>
            </div>

            {/* Conversation Messages Container */}
            <AIConversation
              contentKey={chatMessages.length + (chatMessages[chatMessages.length - 1]?.content.length || 0)}
              className="flex-1 p-4 space-y-4 overflow-y-auto"
            >
              {chatMessages.map((msg) => (
                <div key={msg.id} className="space-y-3">
                  <AIMessage from={msg.role} copyText={msg.content}>
                    {msg.isStreaming ? (
                      <AIResponse text={msg.content} isStreaming={true} />
                    ) : (
                      <div className="text-sm text-zinc-200 leading-relaxed whitespace-pre-wrap">
                        {msg.content}
                      </div>
                    )}
                  </AIMessage>

                  {/* Optional Interactive SmoothUI Approval Card */}
                  {msg.approval && (
                    <AIApproval
                      question={msg.approval.question}
                      options={msg.approval.options}
                      onDecide={(opt) => {
                        handleSendMessage(`Decision: ${opt.label} (${opt.id})`);
                      }}
                      className="my-2"
                    />
                  )}

                  {/* Optional SmoothUI Artifact Preview Frame */}
                  {msg.artifact && (
                    <AIArtifact
                      title={msg.artifact.title}
                      code={<pre className="text-xs p-3 font-mono">{msg.artifact.code}</pre>}
                      preview={
                        <div className="text-xs p-4 bg-zinc-900 rounded whitespace-pre-wrap font-sans">
                          {msg.artifact.code}
                        </div>
                      }
                      copyText={msg.artifact.code}
                      className="my-2"
                    />
                  )}
                </div>
              ))}

              {isAiResponding && chatMessages[chatMessages.length - 1]?.content === '' && (
                <div className="flex items-center gap-2 p-3 rounded-lg bg-zinc-900/60 text-xs text-zinc-400">
                  <AILoader variant="dots" />
                  <span>Gemma is thinking and auditing report context...</span>
                </div>
              )}
            </AIConversation>

            {/* Citations & Sources Card */}
            <div className="px-4 py-2 border-t border-zinc-800/60 bg-zinc-950/40">
              <AISources sources={auditSources} label="Context Sources" />
            </div>

            {/* Quick Suggestion Chips */}
            <div className="p-3 border-t border-zinc-800/80 bg-zinc-950/80">
              <AISuggestions
                suggestions={DEFAULT_SUGGESTIONS}
                onSelect={(suggestion) => handleSendMessage(suggestion.label)}
                label="Suggested Audit Questions"
              />
            </div>

            {/* Prompt Input */}
            <div className="p-3 border-t border-zinc-800 bg-[#090a0f]">
              <AIPromptInput
                placeholder="Ask about discrepancies, policy rules, or draft vendor emails..."
                onSubmit={handleSendMessage}
                onStop={handleStopGeneration}
                disabled={isAiResponding && !chatStreamAbort}
              />
            </div>
          </aside>
        )}
      </div>

      {/* Gemma AI Settings Modal */}
      {isAiModalOpen && (
        <div className="fixed inset-0 z-50 bg-black/80 backdrop-blur-sm flex items-center justify-center p-4">
          <div className="bg-zinc-900 border border-zinc-800 rounded-2xl max-w-md w-full p-6 shadow-2xl space-y-4">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2.5">
                <div className="w-8 h-8 rounded-lg bg-blue-500/20 text-blue-400 flex items-center justify-center">
                  <Key className="w-4 h-4" />
                </div>
                <h3 className="text-base font-bold text-white">Gemma AI Configuration</h3>
              </div>
              <button
                onClick={() => setIsAiModalOpen(false)}
                className="text-zinc-500 hover:text-zinc-300"
              >
                ✕
              </button>
            </div>

            <p className="text-xs text-zinc-400 leading-relaxed">
              Gemini / Gemma API keys unlock vision OCR for scanned photos and the streaming audit chat assistant.
              Keys can be stored locally in your browser or set on the server in <code>backend/.env</code>.
            </p>

            <div className="space-y-1.5">
              <label className="text-xs font-semibold text-zinc-300">Google Gemini API Key</label>
              <input
                type="password"
                placeholder="AIzaSy..."
                value={apiKeyInput}
                onChange={(e) => setApiKeyInput(e.target.value)}
                className="w-full bg-zinc-950 border border-zinc-700 rounded-lg px-3 py-2 text-xs text-zinc-200 focus:outline-none focus:border-blue-500 font-mono"
              />
              <span className="text-[11px] text-zinc-500 block">
                Saved securely in browser localStorage and sent via <code>x-gemini-api-key</code> header.
              </span>
            </div>

            <div className="flex items-center justify-between pt-2 border-t border-zinc-800">
              <button
                onClick={clearApiKey}
                className="text-xs text-rose-400 hover:text-rose-300 font-medium"
              >
                Clear Browser Key
              </button>
              <div className="flex gap-2">
                <button
                  onClick={() => setIsAiModalOpen(false)}
                  className="px-3 py-1.5 rounded-lg text-xs font-medium bg-zinc-800 hover:bg-zinc-700 text-zinc-300"
                >
                  Cancel
                </button>
                <button
                  onClick={saveApiKey}
                  className="px-4 py-1.5 rounded-lg text-xs font-semibold bg-blue-600 hover:bg-blue-500 text-white shadow-md shadow-blue-500/20"
                >
                  Save Key
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
