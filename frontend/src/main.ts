// Strict TypeScript Interfaces matching Rust backend
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
  currency?: string;
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
}

// When served from the backend (same origin), use relative paths.
// When running Vite dev server separately, fall back to localhost:3000.
const API_BASE = window.location.port === '5173'
  ? `http://${window.location.hostname}:3000`
  : '';

// Built-in Sample Datasets for instant testing
const CLEAN_SAMPLE: InvoiceData = {
  invoice_id: 'INV-2026-8801',
  vendor_name: 'Apex Cloud Infrastructure Ltd.',
  invoice_date: '2026-10-24',
  currency: 'USD',
  items: [
    {
      description: 'Primary GPU Cluster Hours (A100 x 8)',
      quantity: 120.0,
      unit_price: 32.5,
      total: 3900.0,
    },
    {
      description: 'High Throughput NVMe Block Storage (5TB)',
      quantity: 5.0,
      unit_price: 95.0,
      total: 475.0,
    },
    {
      description: 'Dedicated Interconnect Bandwidth (Gbps)',
      quantity: 2.0,
      unit_price: 180.0,
      total: 360.0,
    },
  ],
  subtotal: 4735.0,
  tax: 426.15,
  total_amount: 5161.15,
};

const FAULTY_SAMPLE: InvoiceData = {
  invoice_id: 'INV-2026-9902-FAULTY',
  vendor_name: 'N/A',
  invoice_date: '2026-10-24',
  currency: 'USD',
  items: [
    {
      description: 'Offshore Software Consulting Hours',
      quantity: 100.0,
      unit_price: 150.0,
      total: 15000.0,
    },
    {
      description: 'Expedited Security Penetration Audit',
      quantity: 1.0,
      unit_price: 4500.0,
      total: 4500.0,
    },
  ],
  subtotal: 18500.0, // Error: 15000 + 4500 = 19500
  tax: 1500.0,
  total_amount: 25000.0, // Error: 18500 + 1500 = 20000 != 25000, plus >= 10000 threshold & suspicious vendor
};

const INR_SAMPLE: InvoiceData = {
  invoice_id: 'INV-INR-2026-101',
  vendor_name: 'Tata Consultancy & Cloud Ltd.',
  invoice_date: '2026-10-24',
  currency: 'INR',
  items: [
    {
      description: 'Managed Kubernetes Cluster',
      quantity: 1.0,
      unit_price: 150000.0,
      total: 150000.0,
    },
    {
      description: 'High IOPS Database Volumes (NVMe)',
      quantity: 2.0,
      unit_price: 25000.0,
      total: 50000.0,
    },
  ],
  subtotal: 200000.0,
  tax: 36000.0,
  total_amount: 236000.0,
};

// Messy, camelCase, wrapped invoice with string numbers and currency symbols
const MESSY_SAMPLE: Record<string, unknown> = {
  status: 'success',
  document: {
    invoiceId: 'INV-2026-MESSY-77',
    vendorName: 'Global Cloud HyperScale Inc.',
    invoiceDate: '2026-10-24',
    currencyCode: 'INR',
    lineItems: [
      {
        name: 'Cloud TPU v5e Pod Slice Hours',
        qty: '25.0',
        unitPrice: '₹ 1,200.00',
        amount: '₹ 30,000.00',
      },
      {
        name: 'High Performance Lustre File Storage',
        qty: '2',
        unitPrice: '₹ 7,500.00',
        amount: '₹ 15,000.00',
      },
    ],
    subTotal: '₹ 45,000.00',
    taxAmount: '₹ 8,100.00',
    totalAmount: '₹ 53,100.00',
  },
};

// State
let lastAuditedInvoice: InvoiceData | null = null;

// DOM Elements
const healthStatusEl = document.getElementById('healthStatus') as HTMLDivElement;
const healthStatusTextEl = document.getElementById('healthStatusText') as HTMLSpanElement;
const refreshHealthBtn = document.getElementById('refreshHealthBtn') as HTMLButtonElement;

const dropZone = document.getElementById('dropZone') as HTMLDivElement;
const fileInput = document.getElementById('fileInput') as HTMLInputElement;
const testCleanBtn = document.getElementById('testCleanBtn') as HTMLButtonElement;
const testInrBtn = document.getElementById('testInrBtn') as HTMLButtonElement;
const testMessyBtn = document.getElementById('testMessyBtn') as HTMLButtonElement;
const testFaultyBtn = document.getElementById('testFaultyBtn') as HTMLButtonElement;

// AI Settings Modal Elements
const openAiSettingsBtn = document.getElementById('openAiSettingsBtn') as HTMLButtonElement;
const aiStatusTag = document.getElementById('aiStatusTag') as HTMLSpanElement;
const aiModal = document.getElementById('aiModal') as HTMLDivElement;
const closeAiModalBtn = document.getElementById('closeAiModalBtn') as HTMLButtonElement;
const geminiApiKeyInput = document.getElementById('geminiApiKeyInput') as HTMLInputElement;
const ollamaHostInput = document.getElementById('ollamaHostInput') as HTMLInputElement;
const saveAiSettingsBtn = document.getElementById('saveAiSettingsBtn') as HTMLButtonElement;

const toastEl = document.getElementById('toastNotification') as HTMLDivElement;
const toastIconEl = document.getElementById('toastIcon') as HTMLSpanElement;
const toastMsgEl = document.getElementById('toastMessage') as HTMLSpanElement;

const jsonViewer = document.getElementById('jsonViewer') as HTMLElement;
const copyJsonBtn = document.getElementById('copyJsonBtn') as HTMLButtonElement;

const invoiceSummaryBar = document.getElementById('invoiceSummaryBar') as HTMLDivElement;
const summaryId = document.getElementById('summaryId') as HTMLSpanElement;
const summaryVendor = document.getElementById('summaryVendor') as HTMLSpanElement;
const summaryDate = document.getElementById('summaryDate') as HTMLSpanElement;
const summaryTotal = document.getElementById('summaryTotal') as HTMLSpanElement;

const overallStatusBadge = document.getElementById('overallStatusBadge') as HTMLDivElement;
const highRiskCountEl = document.getElementById('highRiskCount') as HTMLDivElement;
const mediumRiskCountEl = document.getElementById('mediumRiskCount') as HTMLDivElement;
const lowRiskCountEl = document.getElementById('lowRiskCount') as HTMLDivElement;
const totalFlagsCountEl = document.getElementById('totalFlagsCount') as HTMLDivElement;
const flagsContainer = document.getElementById('flagsContainer') as HTMLDivElement;

// Helper: Toast Notifications
function showToast(message: string, type: 'success' | 'error' | 'info' = 'info') {
  toastEl.className = `toast-banner toast-${type}`;
  toastMsgEl.textContent = message;
  toastIconEl.textContent = type === 'success' ? '✅' : type === 'error' ? '❌' : 'ℹ️';
  toastEl.classList.remove('hidden');

  setTimeout(() => {
    toastEl.classList.add('hidden');
  }, 5000);
}

// Health Check
async function checkBackendHealth() {
  healthStatusEl.className = 'status-pill status-checking';
  healthStatusTextEl.textContent = 'Connecting...';

  try {
    const res = await fetch(`${API_BASE}/health`, { method: 'GET' });
    if (!res.ok) {
      throw new Error(`HTTP ${res.status}`);
    }
    const data = await res.json();
    healthStatusEl.className = 'status-pill status-online';
    healthStatusTextEl.textContent = `Backend Online (${data.version || 'v0.1.0'})`;
  } catch (err) {
    healthStatusEl.className = 'status-pill status-offline';
    healthStatusTextEl.textContent = 'Backend Offline';
  }
}

// Syntax Highlighting JSON
function syntaxHighlightJson(json: string): string {
  const sanitized = json.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  return sanitized.replace(
    /("(\\u[a-zA-Z0-9]{4}|\\[^u]|[^\\"])*"(\s*:)?|\b(true|false|null)\b|-?\d+(?:\.\d*)?(?:[eE][+\\-]?\d+)?)/g,
    (match) => {
      let cls = 'json-number';
      if (/^"/.test(match)) {
        if (/:$/.test(match)) {
          cls = 'json-key';
        } else {
          cls = 'json-string';
        }
      } else if (/true|false/.test(match)) {
        cls = 'json-boolean';
      } else if (/null/.test(match)) {
        cls = 'json-null';
      }
      return `<span class="${cls}">${match}</span>`;
    }
  );
}

// Render Audit Report
function renderAuditReport(report: AuditReport) {
  lastAuditedInvoice = report.parsed_invoice;

  // Render Metadata Bar
  invoiceSummaryBar.classList.remove('hidden');
  summaryId.textContent = report.invoice_id || 'N/A';
  summaryVendor.textContent = report.vendor_name || 'N/A';
  summaryDate.textContent = report.parsed_invoice.invoice_date || 'N/A';
  const currency = report.parsed_invoice.currency || 'USD';
  summaryTotal.textContent = `${report.parsed_invoice.total_amount.toLocaleString(undefined, {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })} ${currency}`;

  // Render JSON left pane
  const formattedJson = JSON.stringify(report.parsed_invoice, null, 2);
  jsonViewer.innerHTML = `<code class="language-json">${syntaxHighlightJson(formattedJson)}</code>`;

  // Render Counters
  highRiskCountEl.textContent = report.high_risk_count.toString();
  mediumRiskCountEl.textContent = report.medium_risk_count.toString();
  lowRiskCountEl.textContent = report.low_risk_count.toString();
  totalFlagsCountEl.textContent = report.total_flags.toString();

  // Render Status Badge
  overallStatusBadge.className = 'status-badge';
  if (report.overall_status === 'PASSED') {
    overallStatusBadge.classList.add('badge-passed');
    overallStatusBadge.textContent = 'PASSED';
  } else if (report.overall_status === 'REVIEW_RECOMMENDED') {
    overallStatusBadge.classList.add('badge-review');
    overallStatusBadge.textContent = 'REVIEW RECOMMENDED';
  } else {
    overallStatusBadge.classList.add('badge-action');
    overallStatusBadge.textContent = 'ACTION REQUIRED';
  }

  // Render Flags List
  flagsContainer.innerHTML = '';

  if (report.flags.length === 0) {
    flagsContainer.innerHTML = `
      <div class="empty-passed-state">
        <div class="empty-icon">🎉</div>
        <h3>All Verifications Passed</h3>
        <p>No calculation errors, suspicious vendor flags, or threshold warnings were detected for this document.</p>
      </div>
    `;
    return;
  }

  report.flags.forEach((flag) => {
    const card = document.createElement('div');
    const riskClass =
      flag.risk_level === 'HIGH'
        ? 'flag-high'
        : flag.risk_level === 'MEDIUM'
        ? 'flag-medium'
        : 'flag-low';

    const badgeClass =
      flag.risk_level === 'HIGH'
        ? 'risk-badge-high'
        : flag.risk_level === 'MEDIUM'
        ? 'risk-badge-medium'
        : 'risk-badge-low';

    card.className = `flag-card ${riskClass}`;
    card.innerHTML = `
      <div class="flag-header-row">
        <div class="flag-title-group">
          <span class="risk-badge ${badgeClass}">${flag.risk_level}</span>
          <span class="flag-title">${flag.title}</span>
        </div>
        <span class="flag-rule-id">${flag.rule_id}</span>
      </div>
      <p class="flag-desc">${flag.description}</p>
    `;
    flagsContainer.appendChild(card);
  });
}

// API: Send JSON invoice (supports standard or flexible raw objects)
async function auditJsonPayload(invoice: unknown) {
  try {
    const candidateId =
      typeof invoice === 'object' && invoice !== null
        ? (invoice as Record<string, unknown>).invoice_id ||
          (invoice as Record<string, unknown>).invoiceId ||
          'Invoice'
        : 'Invoice';

    showToast(`Sending ${candidateId} to auditor...`, 'info');
    const response = await fetch(`${API_BASE}/api/audit-json`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(invoice),
    });

    if (!response.ok) {
      const errData = await response.json().catch(() => ({}));
      throw new Error(errData.error || `Server responded with ${response.status}`);
    }

    const report: AuditReport = await response.json();
    renderAuditReport(report);
    showToast(`Audit complete for ${report.invoice_id} (${report.overall_status})`, 'success');
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : String(err);
    showToast(`Audit failed: ${message}`, 'error');
  }
}

// API: Upload multipart file (supports .json, .txt, and .pdf via Gemma AI)
async function uploadInvoiceFile(file: File) {
  try {
    const isPdf = file.name.toLowerCase().endsWith('.pdf');
    if (isPdf) {
      showToast(`Scanning PDF document with Gemma AI / Native parser...`, 'info');
    } else {
      showToast(`Uploading and normalizing ${file.name}...`, 'info');
    }

    const formData = new FormData();
    formData.append('file', file, file.name);

    const apiKey = localStorage.getItem('gemini_api_key') || '';
    const ollamaHost = localStorage.getItem('ollama_host') || '';

    const headers: Record<string, string> = {};
    if (apiKey) {
      headers['X-Gemini-API-Key'] = apiKey;
    }
    if (ollamaHost) {
      headers['X-Gemma-Endpoint'] = ollamaHost;
    }

    const response = await fetch(`${API_BASE}/api/upload`, {
      method: 'POST',
      headers,
      body: formData,
    });

    if (!response.ok) {
      const errData = await response.json().catch(() => ({}));
      throw new Error(errData.error || errData.hint || `HTTP ${response.status}`);
    }

    const report: AuditReport = await response.json();
    renderAuditReport(report);
    showToast(`Audit finished for ${file.name}!`, 'success');
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : String(err);
    showToast(`File audit failed: ${message}`, 'error');
  }
}

// AI Settings Handlers
function updateAiStatusBadge() {
  const apiKey = localStorage.getItem('gemini_api_key') || '';
  const ollama = localStorage.getItem('ollama_host') || '';

  if (apiKey) {
    aiStatusTag.textContent = 'Gemma Cloud';
    aiStatusTag.style.background = 'rgba(16, 185, 129, 0.2)';
    aiStatusTag.style.color = '#34d399';
  } else if (ollama) {
    aiStatusTag.textContent = 'Ollama Gemma';
    aiStatusTag.style.background = 'rgba(56, 189, 248, 0.2)';
    aiStatusTag.style.color = '#38bdf8';
  } else {
    aiStatusTag.textContent = 'Native Mode';
    aiStatusTag.style.background = 'rgba(99, 102, 241, 0.2)';
    aiStatusTag.style.color = '#818cf8';
  }
}

function openAiSettings() {
  geminiApiKeyInput.value = localStorage.getItem('gemini_api_key') || '';
  ollamaHostInput.value = localStorage.getItem('ollama_host') || '';
  aiModal.classList.remove('hidden');
}

function closeAiSettings() {
  aiModal.classList.add('hidden');
}

function saveAiSettings() {
  const apiKey = geminiApiKeyInput.value.trim();
  const ollama = ollamaHostInput.value.trim();

  if (apiKey) {
    localStorage.setItem('gemini_api_key', apiKey);
  } else {
    localStorage.removeItem('gemini_api_key');
  }

  if (ollama) {
    localStorage.setItem('ollama_host', ollama);
  } else {
    localStorage.removeItem('ollama_host');
  }

  updateAiStatusBadge();
  closeAiSettings();
  showToast('Gemma AI settings saved successfully!', 'success');
}

// Event Listeners Setup
function initApp() {
  // Initialize AI status badge
  updateAiStatusBadge();

  // Health refresh
  refreshHealthBtn.addEventListener('click', checkBackendHealth);
  checkBackendHealth();

  // Polling health every 30s
  setInterval(checkBackendHealth, 30000);

  // Quick action buttons
  testCleanBtn.addEventListener('click', () => auditJsonPayload(CLEAN_SAMPLE));
  testInrBtn.addEventListener('click', () => auditJsonPayload(INR_SAMPLE));
  testMessyBtn.addEventListener('click', () => auditJsonPayload(MESSY_SAMPLE));
  testFaultyBtn.addEventListener('click', () => auditJsonPayload(FAULTY_SAMPLE));

  // AI Settings Modal listeners
  openAiSettingsBtn.addEventListener('click', openAiSettings);
  closeAiModalBtn.addEventListener('click', closeAiSettings);
  saveAiSettingsBtn.addEventListener('click', saveAiSettings);
  aiModal.addEventListener('click', (e) => {
    if (e.target === aiModal) closeAiSettings();
  });

  // Drag and drop zone
  dropZone.addEventListener('click', () => fileInput.click());

  dropZone.addEventListener('dragover', (e) => {
    e.preventDefault();
    dropZone.classList.add('dragover');
  });

  dropZone.addEventListener('dragleave', () => {
    dropZone.classList.remove('dragover');
  });

  dropZone.addEventListener('drop', (e) => {
    e.preventDefault();
    dropZone.classList.remove('dragover');
    if (e.dataTransfer && e.dataTransfer.files.length > 0) {
      const file = e.dataTransfer.files[0];
      uploadInvoiceFile(file);
    }
  });

  fileInput.addEventListener('change', () => {
    if (fileInput.files && fileInput.files.length > 0) {
      const file = fileInput.files[0];
      uploadInvoiceFile(file);
      fileInput.value = ''; // Reset input
    }
  });

  // Copy JSON button
  copyJsonBtn.addEventListener('click', async () => {
    if (!lastAuditedInvoice) {
      showToast('No invoice payload available to copy', 'info');
      return;
    }
    try {
      await navigator.clipboard.writeText(JSON.stringify(lastAuditedInvoice, null, 2));
      showToast('Invoice JSON copied to clipboard!', 'success');
    } catch {
      showToast('Failed to copy to clipboard', 'error');
    }
  });
}

// Launch
document.addEventListener('DOMContentLoaded', initApp);
