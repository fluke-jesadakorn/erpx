// AI Coverage Audit — DB-as-truth report.
// Cross-references the catalog (informational), ai_assignments (configured),
// and ai_invocations (called). Prints a bucket-grouped report and exits 1
// when there are NEVER_CALLED or UNCONFIGURED rows.
//
// Runbook:
//   node db/audit_ai_coverage.js

const { Client } = require('pg');

// Mirror of app/src/lib/ai/sections.ts SECTION_CATALOG.
// Keep in sync when adding new sections.
const CATALOG = [
  { key: 'staff:ocr',           task: 'vision', label: 'Receipt OCR',                    labelTh: 'Receipt Scan (OCR)' },
  { key: 'staff:submit',        task: 'chat',   label: 'Expense submission helper',      labelTh: 'Expense Form Helper' },
  { key: 'acct:coa-search',     task: 'embed',  label: 'COA semantic mapping',           labelTh: 'Map COA Codes with Semantic' },
  { key: 'acct:queue',          task: 'chat',   label: 'Expense anomaly detection',      labelTh: 'Detect Anomalous Items' },
  { key: 'hod:approve',         task: 'chat',   label: 'Approval comment summarizer',    labelTh: 'Summarize Approver Comments' },
  { key: 'am:review',           task: 'chat',   label: 'Policy recommendation',          labelTh: 'Recommend Policy' },
  { key: 'cfo:cockpit',         task: 'chat',   label: 'Executive narrative',            labelTh: 'Executive Summary Narrative' },
  { key: 'ceo:cockpit',         task: 'chat',   label: 'Board summary',                  labelTh: 'Board Summary' },
  { key: 'ledger:commentary',   task: 'chat',   label: 'GL commentary',                  labelTh: 'GL Line Commentary' },
  { key: 'policy:editor',       task: 'chat',   label: 'Policy linting',                 labelTh: 'Review Policy' },
  { key: 'command:intent',      task: 'chat',   label: 'Command palette intent',         labelTh: 'Predict Command ⌘K' },
  { key: 'notification:digest', task: 'chat',   label: 'Notification digest',            labelTh: 'Notification Digest' },
];

const ACTIVE_WINDOW_DAYS = 7;

const dbConfig = {
  host: process.env.DB_HOST || 'localhost',
  port: parseInt(process.env.DB_PORT || '5432', 10),
  user: process.env.DB_USER || 'contract',
  password: process.env.DB_PASSWORD || 'contractpw',
  database: process.env.DB_NAME || 'world_erp_db',
};

function pad(s, n) {
  s = String(s ?? '—');
  return s.length >= n ? s : s + ' '.repeat(n - s.length);
}

function fmtDate(d) {
  if (!d) return '—';
  return new Date(d).toISOString().replace('T', ' ').slice(0, 16) + 'Z';
}

function fmtAge(d) {
  if (!d) return '—';
  const ms = Date.now() - new Date(d).getTime();
  const m = Math.floor(ms / 60000);
  if (m < 1) return 'just now';
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const dd = Math.floor(h / 24);
  if (dd < 30) return `${dd}d ago`;
  return `${Math.floor(dd / 30)}mo ago`;
}

function colorFor(bucket) {
  switch (bucket) {
    case 'NEVER_CALLED': return '\x1b[31m';
    case 'UNCONFIGURED': return '\x1b[37m';
    case 'DORMANT':      return '\x1b[33m';
    case 'ORPHAN':       return '\x1b[35m';
    case 'ACTIVE':       return '\x1b[32m';
    default:             return '\x1b[0m';
  }
}
const RESET = '\x1b[0m';

async function run() {
  const client = new Client(dbConfig);
  await client.connect();

  // 1. Pull DB truth: every assignment row with aggregated invocation stats.
  const healthRes = await client.query(
    `SELECT section_key, task_type, assignment_enabled,
            provider_name, model_name,
            ok_calls, err_calls, total_calls,
            last_invocation_at, first_invocation_at
     FROM ai_section_health`
  );
  const assignments = healthRes.rows;

  // 2. Pull ORPHAN candidates: any section_key in invocations that isn't
  //    covered by ai_assignments or the catalog.
  const catalogKeys = new Set(CATALOG.map(c => c.key));
  const assignedKeys = new Set(assignments.map(a => a.section_key));
  const orphanRes = await client.query(
    `SELECT section_key, task_type,
            COUNT(*) FILTER (WHERE status='ok')    AS ok_calls,
            COUNT(*) FILTER (WHERE status='error') AS err_calls,
            COUNT(*)                              AS total_calls,
            MAX(created_at) AS last_invocation_at
     FROM ai_invocations
     GROUP BY section_key, task_type
     ORDER BY section_key`
  );
  const orphans = orphanRes.rows.filter(
    r => !assignedKeys.has(r.section_key) && !catalogKeys.has(r.section_key)
  );

  await client.end();

  const cutoff = Date.now() - ACTIVE_WINDOW_DAYS * 86400_000;
  const labelByKey = new Map(CATALOG.map(c => [c.key, c]));

  const buckets = { NEVER_CALLED: [], UNCONFIGURED: [], DORMANT: [], ACTIVE: [], ORPHAN: [] };

  for (const a of assignments) {
    const meta = labelByKey.get(a.section_key) || { label: a.section_key, labelTh: a.section_key };
    const total = parseInt(a.total_calls, 10) || 0;
    const last = a.last_invocation_at ? new Date(a.last_invocation_at).getTime() : 0;
    let bucket;
    if (total === 0)          bucket = 'NEVER_CALLED';
    else if (last >= cutoff)  bucket = 'ACTIVE';
    else                      bucket = 'DORMANT';
    buckets[bucket].push({
      section_key: a.section_key,
      task: a.task_type,
      labelTh: meta.labelTh,
      provider: a.provider_name || '—',
      model: a.model_name || '—',
      ok: parseInt(a.ok_calls, 10) || 0,
      err: parseInt(a.err_calls, 10) || 0,
      total,
      last: a.last_invocation_at,
      first: a.first_invocation_at,
    });
  }

  for (const c of CATALOG) {
    if (!assignedKeys.has(c.key)) {
      buckets.UNCONFIGURED.push({
        section_key: c.key,
        task: c.task,
        labelTh: c.labelTh,
        provider: '—',
        model: '—',
        ok: 0, err: 0, total: 0,
        last: null, first: null,
      });
    }
  }

  for (const o of orphans) {
    buckets.ORPHAN.push({
      section_key: o.section_key,
      task: o.task_type,
      labelTh: '(not in catalog)',
      provider: '—',
      model: '—',
      ok: parseInt(o.ok_calls, 10) || 0,
      err: parseInt(o.err_calls, 10) || 0,
      total: parseInt(o.total_calls, 10) || 0,
      last: o.last_invocation_at,
      first: null,
    });
  }

  const totalCatalog = CATALOG.length;
  const activeCount = buckets.ACTIVE.length;
  const coveragePct = totalCatalog > 0 ? ((activeCount / totalCatalog) * 100).toFixed(1) : '0.0';

  console.log(`\nAI Coverage Audit — ${dbConfig.database} @ ${new Date().toISOString().slice(0, 19)}Z`);
  console.log(`Source of truth: ai_assignments (DB). Catalog is informational. Window: ${ACTIVE_WINDOW_DAYS}d\n`);

  const order = ['NEVER_CALLED', 'UNCONFIGURED', 'DORMANT', 'ORPHAN', 'ACTIVE'];
  const header = `${pad('section_key', 26)}${pad('task', 8)}${pad('provider/model', 28)}${pad('ok/err', 10)}${pad('last', 18)}`;

  for (const b of order) {
    const rows = buckets[b];
    const c = colorFor(b);
    console.log(`${c}${b} (${rows.length})${RESET}`);
    if (rows.length === 0) { console.log('   (none)\n'); continue; }
    console.log(`   ${header}`);
    rows.sort((x, y) => String(x.section_key).localeCompare(String(y.section_key)));
    for (const r of rows) {
      const pm = `${r.provider}/${r.model}`;
      const oe = `${r.ok}/${r.err}`;
      const line = `   ${pad(r.section_key, 26)}${pad(r.task, 8)}${pad(pm, 28)}${pad(oe, 10)}${pad(fmtAge(r.last), 18)}`;
      console.log(`${c}${line}${RESET}`);
    }
    console.log();
  }

  console.log(`Coverage: ${activeCount} / ${totalCatalog} (${coveragePct}%)`);

  const fail = buckets.NEVER_CALLED.length > 0 || buckets.UNCONFIGURED.length > 0;
  if (fail) {
    const parts = [];
    if (buckets.NEVER_CALLED.length) parts.push(`${buckets.NEVER_CALLED.length} NEVER_CALLED`);
    if (buckets.UNCONFIGURED.length) parts.push(`${buckets.UNCONFIGURED.length} UNCONFIGURED`);
    console.log(`\nFAIL: ${parts.join(' + ')} — exiting 1`);
    process.exit(1);
  }
  console.log('\nOK: every catalog section is configured and has been called within the window.');
  process.exit(0);
}

run().catch(err => {
  console.error('Audit failed:', err.message);
  process.exit(2);
});