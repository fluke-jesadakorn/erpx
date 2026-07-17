import 'server-only';
import { aiInvoke } from './router';
import { getReadOnlyPool } from '../db';

export interface SqlAskRequest {
  question: string;
  lang?: 'en' | 'th' | 'de';
}

export interface SqlAskResult {
  question: string;
  sql: string;
  columns: string[];
  rows: Array<Record<string, unknown>>;
  rowCount: number;
  explanation: string;
}

const ALLOWED_TABLES = new Set([
  'folio.expenses',
  'folio.expense_items',
  'folio.slips',
  'folio.users',
  'folio.customers',
  'folio.sales_orders',
  'folio.so_items',
  'folio.waybills',
  'folio.waybill_events',
  'finance.chart_of_accounts',
  'finance.ledger_lines',
  'finance.journal_entries',
  'folio.hr_leave',
  'perm.roles',
  'perm.user_roles',
]);

const ALLOWED_COLUMNS: Record<string, Set<string>> = {
  'folio.expenses': new Set(['id','submitter_id','vendor_name','vendor_address','created_to','created_to_address','transaction_date','subtotal','vat_amount','total_amount','payment_method','status','rejection_reason','created_at','updated_at']),
  'folio.expense_items': new Set(['id','expense_id','description','qty','unit_price','amount','mapped_account_code']),
  'folio.slips': new Set(['id','expense_id','kind','status','file_path','mime_type','bank_name','bank_branch','account_number','account_name','ocr_confidence','uploaded_at']),
  'folio.users': new Set(['id','employee_code','fullname','is_active','secondary_locale','hired_at','created_at']),
  'folio.customers': new Set(['id','code','name','name_th','tax_id','billing_address','shipping_address','contact_name','contact_email','contact_phone','credit_limit_thb','payment_terms','blacklist','is_active']),
  'folio.sales_orders': new Set(['id','so_number','customer_id','sales_rep_id','subtotal','vat_total','total_amount','status','created_at','updated_at']),
  'folio.so_items': new Set(['id','sales_order_id','description','qty','unit_price','vat_amount','line_total','mapped_revenue_account_code']),
  'folio.waybills': new Set(['id','origin','origin_id','fiscal_year','current_stage','current_owner_role','current_owner_user_id','status','vendor_name','total_amount','currency','submitter_id','created_at','updated_at','flagged_reason']),
  'folio.waybill_events': new Set(['id','waybill_id','sequence','kind','stage_from','stage_to','actor_id','actor_role','occurred_at']),
  'finance.chart_of_accounts': new Set(['code','name','name_th','account_type']),
  'finance.ledger_lines': new Set(['id','journal_entry_id','account_code','debit','credit','description']),
  'finance.journal_entries': new Set(['id','entry_date','description','is_draft','finalized_at','finalized_by','expense_id','pr_id','po_id','so_id','step','draft_source']),
  'folio.hr_leave': new Set(['waybill_id','employee_id','leave_type','start_date','end_date','days','reason','medical_cert_note']),
  'perm.roles': new Set(['id','display_name','description','is_system','sort_order','parent_role_id','display_name_th','display_name_de','monthly_budget','head_user_id']),
  'perm.user_roles': new Set(['user_id','role_id','granted_at','granted_by']),
};

const FORBIDDEN_KEYWORDS = /\b(INSERT|UPDATE|DELETE|TRUNCATE|DROP|ALTER|CREATE|GRANT|REVOKE|COPY|VACUUM|REINDEX|CLUSTER|LOCK|CALL|DO\s+\$|EXPLAIN\s+ANALYZE|INTO\s+OUTFILE|LOAD\s+DATA|WITH\s+RECURSIVE\s+.*\bINSERT|pg_read_file|pg_ls_dir)\b/i;

function schemaDigest(): string {
  return Array.from(ALLOWED_TABLES).sort().map(t => {
    const cols = Array.from(ALLOWED_COLUMNS[t] ?? []).sort().join(',');
    return `${t}(${cols})`;
  }).join('\n');
}

function safeParse(s: string): { sql?: string; explanation?: string } | null {
  const m = s.match(/\{[\s\S]*\}/);
  if (!m) return null;
  try { return JSON.parse(m[0]); } catch { return null; }
}

function validateSql(sql: string): { ok: boolean; reason?: string; cleanSql: string } {
  const clean = sql.trim().replace(/;+\s*$/, '');
  if (FORBIDDEN_KEYWORDS.test(clean)) {
    return { ok: false, reason: 'forbidden keyword in SQL', cleanSql: clean };
  }
  const refs = Array.from(clean.matchAll(/\b(?:FROM|JOIN|INTO|UPDATE)\s+([a-z_][a-z0-9_.]*)/gi)).map(m => m[1].toLowerCase());
  if (refs.length === 0) return { ok: false, reason: 'no table references found', cleanSql: clean };
  for (const ref of refs) {
    if (!ALLOWED_TABLES.has(ref)) {
      return { ok: false, reason: `table "${ref}" is not in the allow-list`, cleanSql: clean };
    }
  }
  const colRefs = Array.from(clean.matchAll(/\b([a-z_][a-z0-9_]*)\.([a-z_][a-z0-9_]*)/gi));
  for (const m of colRefs) {
    const t = m[1].toLowerCase();
    const c = m[2].toLowerCase();
    const allowed = ALLOWED_COLUMNS[t];
    if (!allowed) {
      return { ok: false, reason: `column reference uses unknown table "${t}"`, cleanSql: clean };
    }
    if (!allowed.has(c)) {
      return { ok: false, reason: `column "${t}.${c}" is not allowed`, cleanSql: clean };
    }
  }
  return { ok: true, cleanSql: clean };
}

async function explainSql(sql: string): Promise<{ columns: string[]; rows: Array<Record<string, unknown>> }> {
  const pool = getReadOnlyPool();
  const client = await pool.connect();
  try {
    await client.query('SET TRANSACTION READ ONLY');
    await client.query('SET LOCAL statement_timeout = 5000');
    const res = await client.query(sql);
    return { columns: res.fields.map(f => f.name), rows: res.rows as Array<Record<string, unknown>> };
  } finally {
    client.release();
  }
}

export async function askSql(req: SqlAskRequest): Promise<SqlAskResult | null> {
  const lang = req.lang ?? 'en';
  const langLine = lang === 'th'
    ? 'อธิบายสั้น ๆ เป็นภาษาไทย'
    : lang === 'de'
      ? 'Erklären Sie kurz auf Deutsch.'
      : 'Brief explanation in English.';

  const r = await aiInvoke('cockpit:sql', 'chat', {
    systemPrompt: `You generate a single read-only PostgreSQL query against the following allow-listed tables:\n${schemaDigest()}\n\nRules:\n- SELECT only; no INSERT/UPDATE/DELETE.\n- Reference tables as schema.table (e.g. folio.expenses).\n- Include a 100-row LIMIT unless the question requires totals (then SUM/COUNT/AVG without LIMIT is fine).\n- Use only the columns listed.\n- Reply with JSON only: {"sql":"...","explanation":"..."}. ${langLine}`,
    text: req.question,
    temperature: 0,
    maxTokens: 800,
  });

  if (!r.ok || !r.text) return null;
  const parsed = safeParse(r.text);
  if (!parsed || typeof parsed.sql !== 'string') return null;

  const v = validateSql(parsed.sql);
  if (!v.ok) {
    return {
      question: req.question,
      sql: parsed.sql,
      columns: [],
      rows: [],
      rowCount: 0,
      explanation: `[rejected] ${v.reason}`,
    };
  }

  try {
    const result = await explainSql(v.cleanSql);
    return {
      question: req.question,
      sql: v.cleanSql,
      columns: result.columns,
      rows: result.rows,
      rowCount: result.rows.length,
      explanation: parsed.explanation ?? '',
    };
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e);
    return {
      question: req.question,
      sql: v.cleanSql,
      columns: [],
      rows: [],
      rowCount: 0,
      explanation: `[runtime error] ${msg}`,
    };
  }
}