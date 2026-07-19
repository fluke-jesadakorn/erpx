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

const ALLOWED_COLUMNS: Record<string, Set<string>> = {
  'folio.expenses': new Set(['id','submitter_id','vendor_name','transaction_date','subtotal','vat_amount','total_amount','payment_method','status','created_at','updated_at','rejection_reason','disbursed_at','disbursed_by','gl_confirmed_at','gl_confirmed_by','pr_id','po_id','created_to','vendor_address','created_to_address','payee_type','currency_code','fx_rate','branch_id','department_id','tax_code','vendor_id','invoice_number','accounting_reviewed_at']),
  'folio.expense_items': new Set(['id','expense_id','description','qty','unit_price','amount','mapped_account_code']),
  'folio.slips': new Set(['id','expense_id','pr_id','po_id','kind','status','file_path','mime_type','file_size','bank_name','bank_branch','account_number','account_name','ocr_confidence','uploaded_by','uploaded_at','confirmed_at']),
  'folio.users': new Set(['id','employee_code','fullname','is_active','secondary_locale','position','job_description','dept_label','hired_at','created_at','quota_sick','used_sick','quota_annual','used_annual','quota_personal','used_personal']),
  'folio.customers': new Set(['id','code','name','name_th','name_de','tax_id','billing_address','shipping_address','contact_name','contact_email','contact_phone','credit_limit_thb','payment_terms','blacklist','is_active','created_at','updated_at']),
  'folio.sales_orders': new Set(['id','so_number','customer_id','sales_rep_id','status','payment_terms','due_date','invoice_number','invoice_issued_at','subtotal','vat_total','total_amount','currency','branch_id','fx_rate','paid_by_id','paid_at','created_at','updated_at']),
  'folio.so_items': new Set(['id','sales_order_id','description','qty','unit_price','vat_amount','line_total','mapped_revenue_account_code','product_id','reserved_qty','shipped_qty','invoiced_qty','returned_qty']),
  'folio.purchase_requisitions': new Set(['id','requester_id','vendor_name','need_by_date','status','total_estimate','currency','justification','created_at','updated_at','dept_group_id','pr_number','vendor_country']),
  'folio.purchase_orders': new Set(['id','pr_id','po_number','vendor_name','total_amount','currency','status','issued_at','issued_by','settled_at','settled_by','created_at','updated_at','vendor_country','branch_id','fx_rate','vendor_id']),
  'folio.po_items': new Set(['id','po_id','description','qty','unit_price','mapped_account_code','product_id','unit_code','received_qty','invoiced_qty','returned_qty','variance_override_by','variance_override_reason']),
  'folio.waybills': new Set(['id','origin','origin_id','fiscal_year','current_stage','current_owner_role','current_owner_user_id','status','vendor_name','total_amount','currency','submitter_id','created_at','updated_at','flagged_reason']),
  'folio.waybill_events': new Set(['id','waybill_id','sequence','kind','stage_from','stage_to','actor_id','actor_role','occurred_at','payload']),
  'folio.hr_leave': new Set(['waybill_id','employee_id','leave_type','start_date','end_date','days','reason','medical_cert_note']),
  'finance.accounts': new Set(['code','name','name_th','name_de','account_type','normal_side','control_type','active','allow_manual_posting']),
  'finance.journals': new Set(['id','journal_no','status','posting_date','document_date','description','currency_code','fx_rate','source_type','source_id','source_event_key','branch_id','waybill_id','preparer_id','prepared_at','approver_id','approved_at','posted_at','reversal_of_id','created_by','created_at']),
  'finance.journal_lines': new Set(['id','journal_id','line_no','account_code','description','debit_thb','credit_thb','foreign_amount','currency_code','branch_id','department_id','customer_id','vendor_id','employee_id','product_id','warehouse_id','waybill_id','source_document_type','source_document_id']),
  'finance.v_posted_lines': new Set(['journal_id','journal_no','posting_date','document_date','journal_description','source_type','source_id','source_event_key','document_currency','fx_rate','journal_branch_id','line_id','line_no','account_code','account_name','account_name_th','account_type','control_type','description','debit_thb','credit_thb','foreign_amount','currency_code','branch_id','department_id','customer_id','vendor_id','employee_id','product_id','warehouse_id','waybill_id','source_document_type','source_document_id']),
  'finance.branches': new Set(['id','code','name','name_th','tax_branch_code','address','active','created_at']),
  'finance.fiscal_periods': new Set(['id','fiscal_year','period_no','starts_on','ends_on','status','closed_by','closed_at','reopened_by','reopened_at']),
  'finance.currencies': new Set(['code','name','decimals','active']),
  'finance.fx_rates': new Set(['rate_date','currency_code','rate_to_thb','source','approved_by','approved_at']),
  'finance.tax_codes': new Set(['code','name','kind','rate','recoverable_rate','account_code','active']),
  'finance.vendors': new Set(['id','code','name','tax_id','billing_address','payment_terms_days','currency_code','payable_account_code','wht_tax_code','active','created_at','updated_at']),
  'finance.commercial_documents': new Set(['id','document_type','document_no','branch_id','customer_id','vendor_id','source_type','source_id','issue_date','currency_code','fx_rate','subtotal','tax_amount','total_amount','status','issued_by','issued_at','journal_id','created_at']),
  'finance.ar_documents': new Set(['id','document_id','customer_id','branch_id','document_no','document_type','document_date','due_date','currency_code','fx_rate','original_foreign','open_foreign','original_thb','open_thb','status','journal_id','created_at']),
  'finance.ar_allocations': new Set(['id','ar_document_id','receipt_document_id','allocation_date','foreign_amount','functional_amount','wht_amount_thb','realized_fx_thb','journal_id','allocated_by','created_at']),
  'finance.ap_documents': new Set(['id','vendor_id','employee_id','branch_id','document_no','document_type','source_type','source_id','document_date','due_date','currency_code','fx_rate','original_foreign','open_foreign','original_thb','open_thb','status','journal_id','created_at']),
  'finance.ap_allocations': new Set(['id','ap_document_id','payment_document_id','allocation_date','foreign_amount','functional_amount','wht_amount_thb','realized_fx_thb','journal_id','allocated_by','created_at']),
  'finance.bank_accounts': new Set(['id','branch_id','code','bank_name','account_name','account_number_masked','currency_code','gl_account_code','active']),
  'finance.bank_imports': new Set(['id','bank_account_id','file_name','row_count','status','imported_by','imported_at']),
  'finance.bank_transactions': new Set(['id','import_id','bank_account_id','row_no','transaction_date','value_date','description','reference','currency_code','amount','balance','status']),
  'finance.bank_match_groups': new Set(['id','bank_account_id','status','difference_thb','fee_thb','fx_difference_thb','journal_id','confirmed_by','confirmed_at','reopened_by','reopened_at','reopen_reason']),
  'finance.budgets': new Set(['id','name','fiscal_year','branch_id','status','approved_by','approved_at','created_by','created_at']),
  'finance.budget_lines': new Set(['id','budget_id','period_no','account_code','department_id','amount_thb']),
  'finance.v_fx_exposure': new Set(['subledger','currency_code','open_foreign','carrying_thb']),
  'finance.v_control_tieouts': new Set(['total_debit','total_credit','journal_balanced','ar_gl','ar_subledger','ar_tied','ap_gl','ap_subledger','ap_tied','inventory_gl','inventory_subledger','inventory_tied']),
  'inventory.products': new Set(['id','sku','name','name_th','category_id','base_unit','lot_tracked','expiry_tracked','inventory_account_code','revenue_account_code','cogs_account_code','active','created_at','updated_at']),
  'inventory.warehouses': new Set(['id','branch_id','code','name','active','created_at']),
  'inventory.lots': new Set(['id','product_id','lot_no','manufactured_on','expires_on','vendor_id','created_at']),
  'inventory.stock_movements': new Set(['id','movement_no','kind','status','movement_date','source_type','source_id','source_event_key','branch_id','journal_id','reversal_of_id','posted_by','posted_at','created_by','created_at']),
  'inventory.stock_movement_lines': new Set(['id','movement_id','line_no','product_id','quantity','unit_code','unit_cost_thb','extended_cost_thb','from_warehouse_id','from_bin_id','to_warehouse_id','to_bin_id','lot_id','source_line_id']),
  'inventory.stock_balances': new Set(['product_id','warehouse_id','bin_id','lot_id','quantity','avg_cost_thb','updated_at']),
  'inventory.reservations': new Set(['id','product_id','warehouse_id','lot_id','source_type','source_id','quantity','status','expires_at','created_at']),
  'inventory.v_valuation': new Set(['product_id','sku','name','warehouse_id','warehouse_code','bin_id','lot_id','lot_no','expires_on','quantity','avg_cost_thb','value_thb']),
  'perm.roles': new Set(['id','display_name','description','kind','rank','is_system','sort_order','created_at','updated_at','department_id']),
  'perm.user_roles': new Set(['user_id','role_id','role_kind','granted_at','granted_by']),
  'perm.departments': new Set(['id','display_name','head_user_id','is_system','created_at','updated_at']),
  'perm.user_departments': new Set(['user_id','department_id','assigned_at','assigned_by']),
};

type SqlCatalog = Record<string, Set<string>>;

const APP_SCHEMAS = ['finance', 'folio', 'inventory', 'law', 'perm'];
const HIDDEN_COLUMN = /(^|_)(api_key|password|secret|token|embedding|file_data|image_data|binary_data)($|_)/i;
let catalogCache: { expiresAt: number; value: SqlCatalog } | null = null;

const FORBIDDEN_KEYWORDS = /\b(INSERT|UPDATE|DELETE|TRUNCATE|DROP|ALTER|CREATE|GRANT|REVOKE|COPY|VACUUM|REINDEX|CLUSTER|LOCK|CALL|DO\s+\$|EXPLAIN\s+ANALYZE|INTO\s+OUTFILE|LOAD\s+DATA|WITH\s+RECURSIVE\s+.*\bINSERT|pg_read_file|pg_ls_dir)\b/i;

function schemaDigest(catalog: SqlCatalog = ALLOWED_COLUMNS): string {
  return Object.keys(catalog).sort().map(t => {
    const cols = Array.from(catalog[t] ?? []).sort().join(',');
    return `${t}(${cols})`;
  }).join('\n');
}

async function liveCatalog(): Promise<SqlCatalog> {
  if (catalogCache && catalogCache.expiresAt > Date.now()) return catalogCache.value;
  const pool = getReadOnlyPool();
  const r = await pool.query<{ table_schema: string; table_name: string; column_name: string }>(
    `SELECT table_schema, table_name, column_name
       FROM information_schema.columns
      WHERE table_schema = ANY($1::text[])
      ORDER BY table_schema, table_name, ordinal_position`,
    [APP_SCHEMAS],
  );
  const value: SqlCatalog = {};
  for (const row of r.rows) {
    if (HIDDEN_COLUMN.test(row.column_name)) continue;
    const table = `${row.table_schema}.${row.table_name}`;
    (value[table] ??= new Set()).add(row.column_name);
  }
  catalogCache = { expiresAt: Date.now() + 5 * 60_000, value };
  return value;
}

function safeParse(s: string): { sql?: string; explanation?: string } | null {
  const m = s.match(/\{[\s\S]*\}/);
  if (!m) return null;
  try { return JSON.parse(m[0]); } catch { return null; }
}

const RESERVED_ALIAS = new Set(['where','join','left','right','full','inner','outer','cross','on','group','order','limit','offset','having','union','intersect','except','window']);

export function validateSql(
  sql: string,
  catalog: SqlCatalog = ALLOWED_COLUMNS,
): { ok: boolean; reason?: string; cleanSql: string } {
  const clean = sql.trim().replace(/;+\s*$/, '');
  if (!/^\s*(SELECT|WITH)\b/i.test(clean)) return { ok: false, reason: 'query must start with SELECT or WITH', cleanSql: clean };
  if (clean.includes(';')) return { ok: false, reason: 'multiple SQL statements are not allowed', cleanSql: clean };
  if (FORBIDDEN_KEYWORDS.test(clean)) {
    return { ok: false, reason: 'forbidden keyword in SQL', cleanSql: clean };
  }
  if (HIDDEN_COLUMN.test(clean)) {
    return { ok: false, reason: 'query references a non-reportable column', cleanSql: clean };
  }
  if (/\bSELECT\s+(?:DISTINCT\s+)?(?:[a-z_][a-z0-9_]*\.)?\*/i.test(clean)) {
    return { ok: false, reason: 'select explicit columns instead of *', cleanSql: clean };
  }
  const tableMatches = Array.from(clean.matchAll(/\b(?:FROM|JOIN)\s+([a-z_][a-z0-9_]*\.[a-z_][a-z0-9_]*)(?:\s+(?:AS\s+)?([a-z_][a-z0-9_]*))?/gi));
  const refs = tableMatches.map((m) => m[1].toLowerCase());
  if (refs.length === 0) return { ok: false, reason: 'no table references found', cleanSql: clean };
  for (const ref of refs) {
    if (!catalog[ref]) {
      return { ok: false, reason: `table "${ref}" is outside Folio`, cleanSql: clean };
    }
  }
  const fullColRefs = Array.from(clean.matchAll(/\b([a-z_][a-z0-9_]*)\.([a-z_][a-z0-9_]*)\.([a-z_][a-z0-9_]*)\b/gi));
  for (const match of fullColRefs) {
    const table = `${match[1].toLowerCase()}.${match[2].toLowerCase()}`;
    const column = match[3].toLowerCase();
    if (!catalog[table]?.has(column)) {
      return { ok: false, reason: `column "${table}.${column}" is unavailable`, cleanSql: clean };
    }
  }
  const aliases = new Map<string, string>();
  const derivedAliases = new Set(
    Array.from(clean.matchAll(/(?:\bWITH|,)\s*([a-z_][a-z0-9_]*)\s+AS\s*\(/gi)).map((match) => match[1].toLowerCase()),
  );
  Array.from(clean.matchAll(/\)\s+(?:AS\s+)?([a-z_][a-z0-9_]*)\b/gi)).forEach((match) => {
    if (!RESERVED_ALIAS.has(match[1].toLowerCase())) derivedAliases.add(match[1].toLowerCase());
  });
  tableMatches.forEach((match) => {
    const table = match[1].toLowerCase();
    const short = table.split('.').at(-1)!;
    aliases.set(short, table);
    const alias = match[2]?.toLowerCase();
    if (alias && !RESERVED_ALIAS.has(alias)) aliases.set(alias, table);
  });
  const colRefs = Array.from(clean.matchAll(/\b([a-z_][a-z0-9_]*)\.([a-z_][a-z0-9_]*)\b/gi));
  for (const m of colRefs) {
    const qualifier = m[1].toLowerCase();
    const c = m[2].toLowerCase();
    if (catalog[`${qualifier}.${c}`]) continue;
    const table = aliases.get(qualifier);
    const allowed = table ? catalog[table] : undefined;
    if (!allowed && derivedAliases.has(qualifier)) continue;
    if (!allowed) {
      return { ok: false, reason: `column reference uses unknown table or alias "${qualifier}"`, cleanSql: clean };
    }
    if (!allowed.has(c)) {
      return { ok: false, reason: `column "${qualifier}.${c}" is unavailable`, cleanSql: clean };
    }
  }
  return { ok: true, cleanSql: clean };
}

function rawSqlFromQuestion(question: string): string | null {
  const fenced = question.match(/```sql\s*([\s\S]*?)```/i)?.[1]?.trim();
  if (fenced && /^(SELECT|WITH)\b/i.test(fenced)) return fenced;
  const direct = question.trim().match(/^(?:run|execute|query)?\s*:?[\s\n]*((?:SELECT|WITH)\b[\s\S]*)$/i)?.[1]?.trim();
  return direct || null;
}

function knownSql(question: string): { sql: string; explanation: string } | null {
  const q = question.toLowerCase();
  const executivePulse = q.includes('executive pulse') || (
    q.includes('cash') &&
    q.includes('open receivable') &&
    q.includes('open payable') &&
    q.includes('inventory') &&
    q.includes('approval')
  );
  if (executivePulse) {
    return {
      sql: `WITH cash AS (
              SELECT COALESCE(SUM(debit_thb - credit_thb), 0) AS amount
                FROM finance.v_posted_lines
               WHERE control_type IN ('bank', 'cash')
                 AND posting_date <= CURRENT_DATE
            ), receivables AS (
              SELECT COALESCE(SUM(open_thb), 0) AS amount
                FROM finance.ar_documents
               WHERE status IN ('open', 'partially_paid')
            ), payables AS (
              SELECT COALESCE(SUM(open_thb), 0) AS amount
                FROM finance.ap_documents
               WHERE status IN ('open', 'partially_paid')
            ), revenue AS (
              SELECT COALESCE(SUM(credit_thb - debit_thb), 0) AS amount
                FROM finance.v_posted_lines
               WHERE account_type = 'revenue'
                 AND posting_date >= date_trunc('month', CURRENT_DATE)::date
                 AND posting_date <= CURRENT_DATE
            ), stock AS (
              SELECT COALESCE(SUM(value_thb), 0) AS amount
                FROM inventory.v_valuation
            ), stage_counts AS (
              SELECT current_stage, COUNT(*)::bigint AS item_count
                FROM folio.waybills
               WHERE status = 'open'
               GROUP BY current_stage
            ), bottlenecks AS (
              SELECT COALESCE(SUM(item_count), 0)::bigint AS item_count,
                     COALESCE(string_agg(current_stage || ': ' || item_count, ', ' ORDER BY item_count DESC, current_stage), 'No open approvals') AS detail
                FROM stage_counts
            )
            SELECT 'Cash balance' AS metric, cash.amount AS amount_thb, NULL::bigint AS item_count, 'Posted cash and bank control accounts' AS detail FROM cash
            UNION ALL SELECT 'Open receivables', receivables.amount, NULL::bigint, 'Open and partially paid AR' FROM receivables
            UNION ALL SELECT 'Open payables', payables.amount, NULL::bigint, 'Open and partially paid AP' FROM payables
            UNION ALL SELECT 'Current-month revenue', revenue.amount, NULL::bigint, 'Posted revenue this month' FROM revenue
            UNION ALL SELECT 'Inventory value', stock.amount, NULL::bigint, 'Current inventory valuation' FROM stock
            UNION ALL SELECT 'Approval bottlenecks', NULL::numeric, bottlenecks.item_count, bottlenecks.detail FROM bottlenecks`,
      explanation: 'Live executive pulse from posted finance records, controlled AR/AP, inventory valuation, and open Folio approval stages.',
    };
  }
  if (q.includes('sales order') && q.includes('open') && (q.includes('how many') || q.includes('total value'))) {
    return {
      sql: `SELECT COUNT(*)::int AS open_orders,
                   COALESCE(SUM(total_amount * CASE WHEN currency::text = 'THB' THEN 1 ELSE fx_rate END), 0) AS total_value_thb
              FROM folio.sales_orders
             WHERE status NOT IN ('so_draft', 'so_paid', 'rejected', 'completed')`,
      explanation: 'Open sales orders and their functional-currency value from the sales-order database.',
    };
  }
  const cashForecast = q.includes('cash') && (q.includes('forecast') || q.includes('next 30 days'));
  const simulation = q.includes('simulate') || q.includes('simulation') || q.includes('scenario');
  if (cashForecast || simulation) {
    return {
      sql: `WITH cash AS (
              SELECT COALESCE(SUM(CASE WHEN control_type IN ('bank','cash') THEN debit_thb - credit_thb ELSE 0 END), 0) AS current_cash
                FROM finance.v_posted_lines
               WHERE posting_date <= CURRENT_DATE
            ), due_ar AS (
              SELECT COALESCE(SUM(open_thb), 0) AS amount
                FROM finance.ar_documents
               WHERE status IN ('open','partially_paid') AND due_date <= CURRENT_DATE + 30
            ), due_ap AS (
              SELECT COALESCE(SUM(open_thb), 0) AS amount
                FROM finance.ap_documents
               WHERE status IN ('open','partially_paid') AND due_date <= CURRENT_DATE + 30
            ), commitments AS (
              SELECT COALESCE(SUM(total_amount * CASE WHEN currency::text = 'THB' THEN 1 ELSE fx_rate END), 0) AS amount
                FROM folio.purchase_orders
               WHERE status NOT IN ('disbursed','rejected')
            )
            SELECT current_cash,
                   due_ar.amount AS due_ar_30d,
                   due_ap.amount AS due_ap_30d,
                   commitments.amount AS committed_orders,
                   current_cash + due_ar.amount - due_ap.amount - commitments.amount AS baseline_forecast,
                   current_cash + due_ar.amount * 0.80 - due_ap.amount - commitments.amount AS collections_20pct_lower,
                   due_ar.amount * -0.20 AS scenario_change
              FROM cash CROSS JOIN due_ar CROSS JOIN due_ap CROSS JOIN commitments`,
      explanation: 'Posted cash plus due AR, less due AP and open PO commitments; the scenario collects 20% less AR.',
    };
  }
  if ((q.includes('past') || q.includes('history') || q.includes('monthly')) && (q.includes('revenue') || q.includes('performance'))) {
    return {
      sql: `SELECT to_char(posting_date, 'YYYY-MM') AS month,
                   SUM(CASE WHEN account_type = 'revenue' THEN credit_thb - debit_thb ELSE 0 END) AS revenue,
                   SUM(CASE WHEN account_type = 'revenue' THEN credit_thb - debit_thb
                            WHEN account_code LIKE '5101%' THEN credit_thb - debit_thb ELSE 0 END) AS gross_margin,
                   SUM(CASE WHEN account_type = 'revenue' THEN credit_thb - debit_thb
                            WHEN account_type = 'expense' THEN credit_thb - debit_thb ELSE 0 END) AS net_income
              FROM finance.v_posted_lines
             GROUP BY to_char(posting_date, 'YYYY-MM')
             ORDER BY month`,
      explanation: 'Monthly performance from posted journal lines only.',
    };
  }
  if (q.includes('posted') && (q.includes('position') || q.includes('revenue') || q.includes('payable'))) {
    return {
      sql: `WITH actual AS (
              SELECT COALESCE(SUM(CASE WHEN account_type = 'revenue' THEN credit_thb - debit_thb ELSE 0 END), 0) AS revenue,
                     COALESCE(SUM(CASE WHEN account_code LIKE '5101%' THEN debit_thb - credit_thb ELSE 0 END), 0) AS cogs,
                     COALESCE(SUM(CASE WHEN control_type IN ('bank','cash') THEN debit_thb - credit_thb ELSE 0 END), 0) AS cash
                FROM finance.v_posted_lines
               WHERE posting_date <= CURRENT_DATE
            ), ar AS (
              SELECT COALESCE(SUM(open_thb), 0) AS amount FROM finance.ar_documents WHERE status IN ('open','partially_paid')
            ), ap AS (
              SELECT COALESCE(SUM(open_thb), 0) AS amount FROM finance.ap_documents WHERE status IN ('open','partially_paid')
            ), stock AS (
              SELECT COALESCE(SUM(value_thb), 0) AS amount FROM inventory.v_valuation
            )
            SELECT revenue, revenue - cogs AS gross_margin, cash,
                   ar.amount AS accounts_receivable, ap.amount AS accounts_payable,
                   stock.amount AS inventory_value
              FROM actual CROSS JOIN ar CROSS JOIN ap CROSS JOIN stock`,
      explanation: 'Current posted actuals with controlled AR/AP and inventory valuation.',
    };
  }
  return null;
}

async function explainSql(sql: string): Promise<{ columns: string[]; rows: Array<Record<string, unknown>> }> {
  const pool = getReadOnlyPool();
  const client = await pool.connect();
  try {
    await client.query('BEGIN READ ONLY');
    await client.query('SET LOCAL statement_timeout = 5000');
    const res = await client.query(`SELECT * FROM (${sql}) AS folio_ai_result LIMIT 100`);
    await client.query('ROLLBACK');
    return { columns: res.fields.map(f => f.name), rows: res.rows as Array<Record<string, unknown>> };
  } catch (error) {
    await client.query('ROLLBACK').catch(() => undefined);
    throw error;
  } finally {
    client.release();
  }
}

export async function askSql(req: SqlAskRequest): Promise<SqlAskResult | null> {
  const lang = req.lang ?? 'en';
  const fallbackLanguage = lang === 'th' ? 'Thai' : lang === 'de' ? 'German' : 'English';

  const catalog = await liveCatalog().catch(() => ALLOWED_COLUMNS);
  const direct = rawSqlFromQuestion(req.question);
  if (direct) {
    const v = validateSql(direct, catalog);
    if (!v.ok) {
      return {
        question: req.question,
        sql: direct,
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
        explanation: 'Executed directly against the live Folio read replica.',
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

  const known = knownSql(req.question);
  if (known) {
    const result = await explainSql(known.sql);
    return {
      question: req.question,
      sql: known.sql,
      columns: result.columns,
      rows: result.rows,
      rowCount: result.rows.length,
      explanation: known.explanation,
    };
  }

  const r = await aiInvoke('cockpit:sql', 'chat', {
    systemPrompt: `You translate a Folio business question into one raw PostgreSQL query. The live application catalog is:\n${schemaDigest(catalog)}\n\nRules:\n- Produce SELECT or WITH ... SELECT only.\n- Reference every table as schema.table.\n- Use only listed tables and columns.\n- Join the minimum tables needed and never invent a column.\n- Include LIMIT 100 for detail rows; aggregates do not need a limit.\n- Use CURRENT_DATE for relative business dates.\n- Detect the language of the business question and write the explanation in that same language, supporting any language. If the question contains no natural language, use ${fallbackLanguage}.\n- Reply with JSON only: {"sql":"...","explanation":"..."}.`,
    text: req.question,
    temperature: 0,
    maxTokens: 800,
  });

  if (!r.ok || !r.text) return null;
  const parsed = safeParse(r.text);
  if (!parsed || typeof parsed.sql !== 'string') return null;

  const v = validateSql(parsed.sql, catalog);
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
