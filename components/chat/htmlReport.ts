export interface HtmlReportOpts {
  title?: string;
  subtitle?: string;
  moneyKeys?: string[];
  dateKeys?: string[];
  lang?: 'en' | 'th' | 'de';
}

export interface HtmlReportArtifacts {
  html: string;
  css: string;
  js: string;
  full: string;
}

type Row = Record<string, unknown>;

const DEFAULT_MONEY = /(total|amount|sum|avg|vat|subtotal|cost|price|fee|salary|wage|balance|debit|credit|net|cash)/i;
const DEFAULT_DATE = /(date|_at|time|timestamp)/i;
const DEFAULT_COUNT = /(count|num|qty|quantity|employees|days|records|chunks|pages|contracts|vendors|customers|invoices)/i;

function esc(v: unknown): string {
  if (v == null) return '';
  return String(v)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function asNumber(v: unknown): number | null {
  if (v == null || v === '') return null;
  if (typeof v === 'number') return Number.isFinite(v) ? v : null;
  const n = Number(String(v).replace(/[, ฿]/g, ''));
  return Number.isFinite(n) ? n : null;
}

function isMoney(col: string, opts: HtmlReportOpts): boolean {
  if (opts.moneyKeys?.includes(col)) return true;
  return DEFAULT_MONEY.test(col) && !DEFAULT_COUNT.test(col);
}

function isDate(col: string, opts: HtmlReportOpts): boolean {
  if (opts.dateKeys?.includes(col)) return true;
  return DEFAULT_DATE.test(col);
}

function detectKind(col: string, opts: HtmlReportOpts): 'money' | 'count' | 'date' | 'text' {
  if (isMoney(col, opts)) return 'money';
  if (DEFAULT_COUNT.test(col)) return 'count';
  if (isDate(col, opts)) return 'date';
  return 'text';
}

function fmt(v: unknown, kind: 'money' | 'count' | 'date' | 'text', lang: 'en' | 'th' | 'de' = 'en'): string {
  if (v == null || v === '') return '—';
  if (kind === 'money') {
    const n = asNumber(v);
    return n == null ? esc(v) : n.toLocaleString('th-TH', { minimumFractionDigits: 2, maximumFractionDigits: 2 }) + ' ฿';
  }
  if (kind === 'count') {
    const n = asNumber(v);
    return n == null ? esc(v) : n.toLocaleString(lang === 'th' ? 'th-TH' : lang === 'de' ? 'de-DE' : 'en-US');
  }
  if (kind === 'date') {
    const s = String(v);
    const d = new Date(s);
    if (!Number.isNaN(d.getTime())) {
      const dateLocale = lang === 'th' ? 'th-TH' : lang === 'de' ? 'de-DE' : 'en-US';
      return d.toLocaleDateString(dateLocale, { year: 'numeric', month: 'short', day: '2-digit' });
    }
    return esc(v);
  }
  return esc(v);
}

function prettyHeader(col: string): string {
  const k = col.toLowerCase();
  const m: Record<string, string> = {
    vendor_name: 'Vendor',
    vendor_address: 'Address',
    status: 'Status',
    expense_count: 'Records',
    count: 'Records',
    total_amount: 'Total',
    total_vat: 'VAT',
    vat_amount: 'VAT',
    vat_total: 'VAT',
    vat: 'VAT',
    subtotal: 'Subtotal',
    amount: 'Amount',
    sum: 'Total',
    avg: 'Average',
    total: 'Total',
    category: 'Category',
    category_name: 'Category',
    account_type: 'Type',
    account_code: 'Code',
    description: 'Description',
    transaction_date: 'Date',
    created_at: 'Created',
    payment_method: 'Method',
    submitter_name: 'Submitter',
    id: 'ID',
    employee_count: 'Employees',
    total_employees: 'Employees',
    active_employees: 'Active',
    employee_code: 'Code',
    department: 'Department',
    position: 'Position',
    leave_type: 'Leave Type',
    leave_days: 'Days',
    doc_no: 'Doc No',
    file_name: 'File',
    file_type: 'Type',
    chunk_count: 'Chunks',
    page_count: 'Pages',
    size_bytes: 'Size',
    uploaded_at: 'Uploaded',
    storage_bucket: 'Bucket',
    storage_path: 'Path',
    error_message: 'Error',
    line_user_id: 'LINE User',
    token_count: 'Tokens',
    chunk_index: 'Chunk',
    page_index: 'Page',
  };
  return m[k] ?? col.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
}

export function buildHtmlReport(
  columns: string[],
  rows: Row[],
  opts: HtmlReportOpts = {},
): HtmlReportArtifacts {
  const kinds = columns.map((c) => detectKind(c, opts));
  const title = opts.title ?? 'Query result';
  const subtitle = opts.subtitle ?? `${rows.length} ${rows.length === 1 ? 'row' : 'rows'}`;
  const totalByKind: Record<string, number> = {};
  const hasTotals = kinds.some((k) => k === 'money');
  if (hasTotals) {
    columns.forEach((c, i) => {
      if (kinds[i] !== 'money') return;
      const s = rows.reduce((acc, r) => acc + (asNumber(r[c]) ?? 0), 0);
      totalByKind[c] = s;
    });
  }

  const head = columns
    .map((c, i) => `<th class="col-${kinds[i]}" data-kind="${kinds[i]}" data-col="${esc(c)}">${esc(prettyHeader(c))}</th>`)
    .join('');

  const body = rows
    .map((r, idx) => {
      const tds = columns
        .map((c, i) => `<td class="col-${kinds[i]}" data-kind="${kinds[i]}">${fmt(r[c], kinds[i], opts.lang ?? 'en')}</td>`)
        .join('');
      return `<tr data-idx="${idx}">${tds}</tr>`;
    })
    .join('');

  const totalsRow = hasTotals
    ? `<tr class="totals-row">${columns
        .map((c, i) => {
          if (kinds[i] === 'money') {
            return `<td class="col-money"><strong>${fmt(totalByKind[c], 'money', opts.lang ?? 'en')}</strong></td>`;
          }
          return `<td class="col-${kinds[i]}"></td>`;
        })
        .join('')}</tr>`
    : '';

  const css = `* { box-sizing: border-box; }
:root { color-scheme: dark; }
body { margin: 0; padding: 0; font: 13px/1.5 -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif; background: #0f172a; color: #e2e8f0; }
.wrap { padding: 14px 16px; }
.hdr { display: flex; align-items: center; gap: 10px; margin-bottom: 10px; }
.hdr h1 { font-size: 14px; margin: 0; letter-spacing: .03em; }
.hdr .sub { color: #94a3b8; font-size: 11px; }
.hdr .tag { display: inline-block; padding: 2px 8px; border-radius: 999px; background: rgba(34,211,238,.18); color: #67e8f9; font-size: 10px; letter-spacing: .08em; font-family: ui-monospace, monospace; }
.tools { display: flex; gap: 6px; margin-left: auto; }
.tools button { background: transparent; border: 1px solid #334155; color: #cbd5e1; padding: 3px 8px; border-radius: 4px; font-size: 11px; cursor: pointer; }
.tools button:hover { background: #1e293b; color: #f1f5f9; }
.search { width: 100%; padding: 6px 10px; margin: 8px 0 10px; border: 1px solid #1e293b; border-radius: 4px; background: #0b1220; color: #e2e8f0; font-size: 12px; outline: none; }
.search:focus { border-color: #22d3ee; }
.tbl { width: 100%; border-collapse: separate; border-spacing: 0; }
.tbl thead th { position: sticky; top: 0; background: #0b1220; color: #cbd5e1; font-weight: 600; text-align: left; padding: 8px 12px; border-bottom: 1px solid #1e293b; cursor: pointer; user-select: none; font-size: 11px; letter-spacing: .04em; text-transform: uppercase; }
.tbl thead th:hover { color: #22d3ee; }
.tbl thead th.sorted-asc::after { content: ' ↑'; color: #22d3ee; }
.tbl thead th.sorted-desc::after { content: ' ↓'; color: #22d3ee; }
.tbl tbody td { padding: 7px 12px; border-bottom: 1px solid #111827; vertical-align: middle; }
.tbl tbody tr:hover { background: rgba(34,211,238,.06); }
.col-money, .col-count { text-align: right; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-variant-numeric: tabular-nums; }
.col-date { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 11px; color: #94a3b8; }
.tbl tfoot td { padding: 8px 12px; border-top: 2px solid #334155; background: #0b1220; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-variant-numeric: tabular-nums; }
.tbl tfoot strong { color: #fbbf24; }
.empty { padding: 30px; text-align: center; color: #64748b; font-style: italic; }
.col-money { color: #cbd5e1; }
@media (max-width: 480px) { .tools { flex-wrap: wrap; } .tools button { padding: 2px 6px; } }`;

  const html = `<div class="wrap">
<div class="hdr">
  <span class="tag">REPORT</span>
  <h1>${esc(title)}</h1>
  <span class="sub">${esc(subtitle)}</span>
  <div class="tools">
    <button type="button" data-act="copy">⧉ copy HTML</button>
    <button type="button" data-act="download">↓ .html</button>
  </div>
</div>
<input class="search" placeholder="Filter rows…" />
${rows.length === 0 ? '<div class="empty">No rows</div>' : `
<table class="tbl">
  <thead><tr>${head}</tr></thead>
  <tbody>${body}</tbody>
  ${hasTotals ? `<tfoot>${totalsRow}</tfoot>` : ''}
</table>`}
</div>`;

  const js = `(function(){
  var root = document;
  var tbl = document.querySelector('.tbl');
  if (!tbl) return;
  var tbody = tbl.querySelector('tbody');
  var search = document.querySelector('.search');
  var sortIdx = -1, sortDir = 1;
  function numeric(v){ if(v==null) return -Infinity; var n=Number(String(v).replace(/[, ฿]/g,'')); return isFinite(n)?n:NaN; }
  function reapply(){
    var q = (search && search.value || '').toLowerCase();
    Array.prototype.forEach.call(tbody.querySelectorAll('tr'), function(tr){
      var txt = tr.textContent.toLowerCase();
      tr.style.display = !q || txt.indexOf(q) >= 0 ? '' : 'none';
    });
  }
  function resort(){
    if (sortIdx < 0) return;
    var th = tbl.querySelectorAll('thead th')[sortIdx];
    var kind = th.getAttribute('data-kind');
    var rows = Array.prototype.slice.call(tbody.querySelectorAll('tr'));
    rows.sort(function(a,b){
      var av = a.children[sortIdx].textContent;
      var bv = b.children[sortIdx].textContent;
      if (kind === 'money' || kind === 'count') {
        var an = numeric(av), bn = numeric(bv);
        if (!isNaN(an) && !isNaN(bn)) return (an-bn)*sortDir;
      }
      return av.localeCompare(bv) * sortDir;
    });
    rows.forEach(function(r){ tbody.appendChild(r); });
    tbl.querySelectorAll('thead th').forEach(function(t,i){
      t.classList.remove('sorted-asc','sorted-desc');
      if (i === sortIdx) t.classList.add(sortDir === 1 ? 'sorted-asc' : 'sorted-desc');
    });
  }
  tbl.addEventListener('click', function(e){
    var th = e.target.closest && e.target.closest('th');
    if (!th || !tbl.querySelector('thead').contains(th)) return;
    var idx = Array.prototype.indexOf.call(th.parentNode.children, th);
    if (sortIdx === idx) sortDir = -sortDir; else { sortIdx = idx; sortDir = 1; }
    resort();
  });
  if (search) search.addEventListener('input', reapply);
  document.addEventListener('click', function(e){
    var b = e.target.closest && e.target.closest('button[data-act]');
    if (!b) return;
    var act = b.getAttribute('data-act');
    if (act === 'copy') {
      var html = document.documentElement.outerHTML;
      navigator.clipboard && navigator.clipboard.writeText(html);
      b.textContent = '✓ copied';
      setTimeout(function(){ b.textContent = '⧉ copy HTML'; }, 1200);
    } else if (act === 'download') {
      var blob = new Blob([document.documentElement.outerHTML], { type: 'text/html' });
      var a = document.createElement('a');
      a.href = URL.createObjectURL(blob);
      a.download = 'report.html';
      a.click();
      setTimeout(function(){ URL.revokeObjectURL(a.href); }, 500);
    }
  });
})();`;

  const full = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; img-src data: blob:; connect-src 'none'" />
<title>${esc(title)}</title>
<style>${css}</style>
</head>
<body>${html}<script>${js}</script></body>
</html>`;

  return { html, css, js, full };
}
