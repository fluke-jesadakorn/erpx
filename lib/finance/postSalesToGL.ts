import 'server-only';
import { query, withTransaction } from '../db';
import { loadPostingActor, postJournalInTransaction, type FinanceQuery } from './journals';

export type SalesStep = 'sales_vat' | 'sales_accrual' | 'sales_settlement';

export type Locale = 'th' | 'de';

export interface SalesGlLine {
  account_code: string;
  account_name?: string | null;
  account_name_th?: string | null;
  debit: number;
  credit: number;
  description: string;
}

export interface SalesGlResult {
  journalId: number;
  lines: SalesGlLine[];
  metadata: { note: string | null };
}

interface SalesGlConfig {
  vatAccountCode: string;
  arAccountCode: string;
  cashAccountCode: string;
  revenueAccountCode: string;
}

const T = {
  vatPayable: {
    en: (id: number) => `SO-${id} VAT payable`,
    th: (id: number) => `SO-${id} ภาษีขายค้างจ่าย`,
    de: (id: number) => `SO-${id} USt-Verbindlichkeit`,
  },
  revenue: {
    en: (id: number) => `SO-${id} revenue`,
    th: (id: number) => `SO-${id} รายได้จากการขาย`,
    de: (id: number) => `SO-${id} Umsatzerlöse`,
  },
  arAccrual: {
    en: (id: number, total: string) => `SO-${id} AR — ${total} THB`,
    th: (id: number, total: string) => `SO-${id} ลูกหนี้การค้า — ${total} บาท`,
    de: (id: number, total: string) => `SO-${id} Forderungen — ${total} THB`,
  },
  cashIn: {
    en: (id: number) => `SO-${id} AR receipt — cash in`,
    th: (id: number) => `SO-${id} รับเงิน — เงินสดเข้าบัญชี`,
    de: (id: number) => `SO-${id} Forderungseingang — Barzahlung`,
  },
  arClear: {
    en: (id: number) => `SO-${id} clear AR — cash received`,
    th: (id: number) => `SO-${id} ตัดลูกหนี้ — เงินสดรับแล้ว`,
    de: (id: number) => `SO-${id} Forderungen ausgeglichen — Bar erhalten`,
  },
  draftDesc: {
    en: (vendor: string, step: SalesStep, id: number) => `DRAFT ${step}: ${vendor} (SO-${id})`,
    th: (vendor: string, step: SalesStep, id: number) => `ร่าง ${step}: ${vendor} (SO-${id})`,
    de: (vendor: string, step: SalesStep, id: number) => `ENTWURF ${step}: ${vendor} (SO-${id})`,
  },
};

const ACCOUNT_FALLBACK = {
  outputVatPayable: {
    en: 'Output VAT Payable',
    th: 'ภาษีขาย',
    de: 'Verbindlichkeit USt',
  },
  revenue: {
    en: 'Sales Revenue',
    th: 'รายได้จากการขาย',
    de: 'Umsatzerlöse',
  },
  ar: {
    en: 'Accounts Receivable',
    th: 'ลูกหนี้การค้า',
    de: 'Forderungen aus Lieferungen und Leistungen',
  },
  cash: {
    en: 'Cash at Bank',
    th: 'เงินฝากธนาคาร',
    de: 'Bankguthaben',
  },
};

async function loadConfig(salesOrderId: number): Promise<SalesGlConfig> {
  const r = await query<{
    vat_account_code: string;
    ar_account_code: string;
    cash_account_code: string;
    revenue_account_code: string;
  }>(
    `SELECT vat_account_code, ar_account_code, cash_account_code, revenue_account_code
       FROM sales_orders
      WHERE id = $1`,
    [salesOrderId],
  );
  const row = r.rows[0];
  return {
    vatAccountCode: row?.vat_account_code ?? '210300',
    arAccountCode: row?.ar_account_code ?? '110400',
    cashAccountCode: row?.cash_account_code ?? '110200',
    revenueAccountCode: row?.revenue_account_code ?? '410100',
  };
}

async function loadCoaNames(codes: string[]): Promise<Map<string, { name: string | null; name_th: string | null }>> {
  if (codes.length === 0) return new Map();
  const r = await query<{ code: string; name: string | null; name_th: string | null }>(
    `SELECT code, name, name_th FROM chart_of_accounts WHERE code = ANY($1::text[])`,
    [codes],
  );
  return new Map(r.rows.map((a) => [a.code, a]));
}

async function loadTotals(salesOrderId: number): Promise<{ subtotal: number; vat: number; total: number }> {
  const r = await query<{ subtotal: string; vat_total: string; total_amount: string }>(
    `SELECT subtotal::text, vat_total::text, total_amount::text
       FROM sales_orders
      WHERE id = $1`,
    [salesOrderId],
  );
  const row = r.rows[0];
  return {
    subtotal: row ? parseFloat(row.subtotal) : 0,
    vat: row ? parseFloat(row.vat_total) : 0,
    total: row ? parseFloat(row.total_amount) : 0,
  };
}

function fmtMoneyForLocale(n: number, locale: Locale): string {
  const intl = new Intl.NumberFormat(locale === 'de' ? 'de-DE' : 'th-TH', {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
  return intl.format(n);
}

async function buildAccrualLines(
  salesOrderId: number,
  cfg: SalesGlConfig,
  locale: Locale,
): Promise<{ lines: SalesGlLine[]; note: string | null }> {
  const items = await query<{
    id: number;
    line_total: string;
    vat_amount: string;
    description: string;
    mapped_revenue_account_code: string | null;
    account_name: string | null;
    account_name_th: string | null;
  }>(
    `SELECT i.id,
            i.line_total::text,
            i.vat_amount::text,
            i.description,
            i.mapped_revenue_account_code,
            c.name AS account_name,
            c.name_th AS account_name_th
       FROM so_items i
       LEFT JOIN chart_of_accounts c ON c.code = i.mapped_revenue_account_code
      WHERE i.sales_order_id = $1
   ORDER BY i.id ASC`,
    [salesOrderId],
  );

  let unmappedCount = 0;
  let allMapped = items.rows.length > 0;
  for (const it of items.rows) {
    if (!it.mapped_revenue_account_code) {
      unmappedCount++;
      allMapped = false;
    }
  }
  const note: string | null = allMapped
    ? 'all_mapped_by_ai'
    : unmappedCount > 0
      ? `unmapped_so_lines:${unmappedCount}`
      : null;

  const coaCodes = new Set<string>([cfg.arAccountCode]);
  for (const it of items.rows) {
    if (it.mapped_revenue_account_code) coaCodes.add(it.mapped_revenue_account_code);
    else coaCodes.add(cfg.revenueAccountCode);
  }
  const coaMap = await loadCoaNames(Array.from(coaCodes));
  const acct = coaMap.get(cfg.arAccountCode);

  const revenueByAccount = new Map<string, { amount: number; description: string | null; accountName: string | null; accountNameTh: string | null }>();
  for (const it of items.rows) {
    const amt = parseFloat(it.line_total) - parseFloat(it.vat_amount);
    if (amt <= 0) continue;
    const code = it.mapped_revenue_account_code ?? cfg.revenueAccountCode;
    const cur = revenueByAccount.get(code) ?? { amount: 0, description: it.description, accountName: it.account_name, accountNameTh: it.account_name_th };
    cur.amount += amt;
    revenueByAccount.set(code, cur);
  }

  const totals = await loadTotals(salesOrderId);
  const lines: SalesGlLine[] = [];
  const fallbackRevDesc =
    locale === 'de'
      ? T.revenue.de(salesOrderId)
      : locale === 'th'
        ? T.revenue.th(salesOrderId)
        : T.revenue.en(salesOrderId);
  for (const [code, agg] of revenueByAccount) {
    const meta = coaMap.get(code);
    lines.push({
      account_code: code,
      account_name: agg.accountName ?? meta?.name ?? null,
      account_name_th: agg.accountNameTh ?? meta?.name_th ?? null,
      debit: 0,
      credit: agg.amount,
      description: agg.description ?? fallbackRevDesc,
    });
  }
  if (totals.vat > 0) {
    const vat = coaMap.get(cfg.vatAccountCode);
    lines.push({
      account_code: cfg.vatAccountCode,
      account_name: vat?.name ?? ACCOUNT_FALLBACK.outputVatPayable.en,
      account_name_th: vat?.name_th ?? ACCOUNT_FALLBACK.outputVatPayable.th,
      debit: 0,
      credit: totals.vat,
      description: locale === 'de' ? T.vatPayable.de(salesOrderId) : locale === 'th' ? T.vatPayable.th(salesOrderId) : T.vatPayable.en(salesOrderId),
    });
  }
  const totalFmt = fmtMoneyForLocale(totals.total, locale);
  const arDesc =
    locale === 'de'
      ? T.arAccrual.de(salesOrderId, totalFmt)
      : locale === 'th'
        ? T.arAccrual.th(salesOrderId, totalFmt)
        : T.arAccrual.en(salesOrderId, totalFmt);
  const arName =
    locale === 'de'
      ? ACCOUNT_FALLBACK.ar.de
      : locale === 'th'
        ? ACCOUNT_FALLBACK.ar.th
        : ACCOUNT_FALLBACK.ar.en;
  lines.push({
    account_code: cfg.arAccountCode,
    account_name: acct?.name ?? ACCOUNT_FALLBACK.ar.en,
    account_name_th: arName,
    debit: totals.total,
    credit: 0,
    description: arDesc,
  });

  return { lines, note };
}

async function buildSettlementLines(
  salesOrderId: number,
  cfg: SalesGlConfig,
  locale: Locale,
): Promise<SalesGlLine[]> {
  const totals = await loadTotals(salesOrderId);
  const total = totals.total;
  if (total <= 0) return [];
  const coaMap = await loadCoaNames([cfg.arAccountCode, cfg.cashAccountCode]);
  const ar = coaMap.get(cfg.arAccountCode);
  const cb = coaMap.get(cfg.cashAccountCode);
  const cashDesc =
    locale === 'de'
      ? T.cashIn.de(salesOrderId)
      : locale === 'th'
        ? T.cashIn.th(salesOrderId)
        : T.cashIn.en(salesOrderId);
  const clearDesc =
    locale === 'de'
      ? T.arClear.de(salesOrderId)
      : locale === 'th'
        ? T.arClear.th(salesOrderId)
        : T.arClear.en(salesOrderId);
  const cashName =
    locale === 'de'
      ? ACCOUNT_FALLBACK.cash.de
      : locale === 'th'
        ? ACCOUNT_FALLBACK.cash.th
        : ACCOUNT_FALLBACK.cash.en;
  const arName =
    locale === 'de'
      ? ACCOUNT_FALLBACK.ar.de
      : locale === 'th'
        ? ACCOUNT_FALLBACK.ar.th
        : ACCOUNT_FALLBACK.ar.en;
  return [
    {
      account_code: cfg.cashAccountCode,
      account_name: cb?.name ?? ACCOUNT_FALLBACK.cash.en,
      account_name_th: cashName,
      debit: total,
      credit: 0,
      description: cashDesc,
    },
    {
      account_code: cfg.arAccountCode,
      account_name: ar?.name ?? ACCOUNT_FALLBACK.ar.en,
      account_name_th: arName,
      debit: 0,
      credit: total,
      description: clearDesc,
    },
  ];
}

async function insertLines(journalId: number, lines: SalesGlLine[]): Promise<void> {
  for (const ln of lines) {
    await query(
      `INSERT INTO ledger_lines (journal_entry_id, account_code, debit, credit, description)
       VALUES ($1, $2, $3, $4, $5)`,
      [journalId, ln.account_code, ln.debit, ln.credit, ln.description],
    );
  }
}

async function upsertDraftStep(
  salesOrderId: number,
  step: SalesStep,
  buildLines: () => Promise<SalesGlLine[]>,
  vendorName: string,
  locale: Locale,
  metadataNote?: string | null,
): Promise<SalesGlResult> {
  const journalId = await withTransaction(async (q) => {
    const existing = await q<{ id: number }>(
      `SELECT id FROM journal_entries
        WHERE so_id = $1 AND step = $2 AND is_draft = TRUE
        LIMIT 1`,
      [salesOrderId, step],
    );
    let id: number;
    if (existing.rows[0]) {
      id = existing.rows[0].id;
    } else {
      const baseDesc =
        locale === 'de'
          ? T.draftDesc.de(vendorName, step, salesOrderId)
          : locale === 'th'
            ? T.draftDesc.th(vendorName, step, salesOrderId)
            : T.draftDesc.en(vendorName, step, salesOrderId);
      const desc = metadataNote
        ? `${baseDesc} · [note: ${metadataNote}]`
        : baseDesc;
      const ins = await q<{ id: number }>(
        `INSERT INTO journal_entries (so_id, description, is_draft, draft_source, step)
         VALUES ($1, $2, TRUE, 'so', $3)
         RETURNING id`,
        [salesOrderId, desc, step],
      );
      id = ins.rows[0].id;
    }
    await q(`DELETE FROM ledger_lines WHERE journal_entry_id = $1`, [id]);
    return id;
  });
  const lines = await buildLines();
  if (lines.length > 0) await insertLines(journalId, lines);
  return { journalId, lines, metadata: { note: metadataNote ?? null } };
}

export async function upsertSalesDraftVat(args: {
  salesOrderId: number;
  vendorName: string;
  locale?: Locale;
}): Promise<SalesGlResult> {
  return upsertSalesDraftAccrual(args);
}

export async function upsertSalesDraftAccrual(args: {
  salesOrderId: number;
  vendorName: string;
  locale?: Locale;
}): Promise<SalesGlResult> {
  const locale: Locale = args.locale ?? 'th';
  const cfg = await loadConfig(args.salesOrderId);
  let capturedNote: string | null | undefined;
  return upsertDraftStep(
    args.salesOrderId,
    'sales_accrual',
    async () => {
      const { lines, note } = await buildAccrualLines(args.salesOrderId, cfg, locale);
      capturedNote = note;
      return lines;
    },
    args.vendorName,
    locale,
    capturedNote,
  );
}

export async function upsertSalesDraftSettlement(args: {
  salesOrderId: number;
  vendorName: string;
  locale?: Locale;
}): Promise<SalesGlResult> {
  const locale: Locale = args.locale ?? 'th';
  const cfg = await loadConfig(args.salesOrderId);
  return upsertDraftStep(
    args.salesOrderId,
    'sales_settlement',
    () => buildSettlementLines(args.salesOrderId, cfg, locale),
    args.vendorName,
    locale,
  );
}

export async function finalizeSalesDraft(args: {
  journalId: number;
  actorId: number;
}): Promise<{ journalId: number } | null> {
  return withTransaction(async (q) => {
    const r = await q<{
      id: number;
      so_id: number;
      step: SalesStep;
      description: string;
      so_number: string;
      invoice_number: string | null;
      invoice_issued_at: string | null;
      customer_id: number;
      branch_id: string;
      currency: string;
      fx_rate: string;
      subtotal: string;
      vat_total: string;
      total_amount: string;
      due_date: string | null;
    }>(
      `SELECT j.id, j.so_id, j.step, j.description, so.so_number, so.invoice_number,
              so.invoice_issued_at::text, so.customer_id, so.branch_id::text,
              so.currency, so.fx_rate::text, so.subtotal::text, so.vat_total::text,
              so.total_amount::text, so.due_date::text
         FROM journal_entries j
         JOIN sales_orders so ON so.id = j.so_id
        WHERE j.id = $1 AND j.is_draft = TRUE AND j.draft_source = 'so'
        FOR UPDATE OF j`,
      [args.journalId],
    );
    const draft = r.rows[0];
    if (!draft) return null;
    const actor = await loadPostingActor(q as FinanceQuery, args.actorId);
    const step = draft.step === 'sales_vat' ? 'sales_accrual' : draft.step;
    const fx = draft.currency.trim() === 'THB' ? 1 : Number(draft.fx_rate);
    const existing = await q<{ id: string }>(
      `SELECT id::text FROM finance.journals WHERE source_event_key = $1`,
      [`sales:${draft.so_id}:${step}:v1`],
    );
    let officialId = existing.rows[0] ? Number(existing.rows[0].id) : 0;
    if (!officialId && step === 'sales_accrual') {
      const postingDate = new Date().toISOString().slice(0, 10);
      const documentDate = (draft.invoice_issued_at ?? postingDate).slice(0, 10);
      const legacyLines = await q<SalesGlLine>(
        `SELECT account_code, debit::float8 AS debit, credit::float8 AS credit,
                coalesce(description, '') AS description
           FROM ledger_lines WHERE journal_entry_id = $1 ORDER BY id`,
        [draft.id],
      );
      const official = await postJournalInTransaction(q as FinanceQuery, {
        postingDate,
        documentDate,
        description: `Sales invoice ${draft.invoice_number ?? draft.so_number}`,
        currencyCode: draft.currency.trim(),
        fxRate: fx,
        sourceType: 'sales_invoice',
        sourceId: String(draft.so_id),
        sourceEventKey: `sales:${draft.so_id}:sales_accrual:v1`,
        branchId: Number(draft.branch_id),
        lines: legacyLines.rows.map((line) => ({
          accountCode: line.account_code,
          description: line.description,
          debitThb: Math.round(line.debit * fx * 100) / 100,
          creditThb: Math.round(line.credit * fx * 100) / 100,
          foreignAmount: line.debit > 0 ? line.debit : -line.credit,
          currencyCode: draft.currency.trim(),
          branchId: Number(draft.branch_id),
          customerId: draft.customer_id,
          waybillId: null,
          sourceDocumentType: 'invoice',
          sourceDocumentId: draft.invoice_number ?? draft.so_number,
        })),
      }, actor);
      officialId = official.id;
      const documentNo = draft.invoice_number ?? (await q<{ document_no: string }>(
        `SELECT finance.next_document_number('INV', $1, $2::date) AS document_no`,
        [Number(draft.branch_id), (draft.invoice_issued_at ?? new Date().toISOString()).slice(0, 10)],
      )).rows[0].document_no;
      const document = await q<{ id: string }>(
        `INSERT INTO finance.commercial_documents
           (document_type, document_no, branch_id, customer_id, source_type, source_id,
            issue_date, currency_code, fx_rate, subtotal, tax_amount, total_amount,
            status, issued_by, issued_at, journal_id)
         VALUES ('invoice',$1,$2,$3,'sales_order',$4,$5,$6,$7,$8,$9,$10,'issued',$11,now(),$12)
         ON CONFLICT (document_no) DO UPDATE SET journal_id = excluded.journal_id
         RETURNING id::text`,
        [documentNo, Number(draft.branch_id), draft.customer_id, String(draft.so_id), (draft.invoice_issued_at ?? new Date().toISOString()).slice(0, 10), draft.currency.trim(), fx, Number(draft.subtotal), Number(draft.vat_total), Number(draft.total_amount), args.actorId, officialId],
      );
      const totalThb = Math.round(Number(draft.total_amount) * fx * 100) / 100;
      await q(
        `INSERT INTO finance.ar_documents
           (document_id, customer_id, branch_id, document_no, document_type,
            document_date, due_date, currency_code, fx_rate, original_foreign,
            open_foreign, original_thb, open_thb, journal_id)
         VALUES ($1,$2,$3,$4,'invoice',$5,$6,$7,$8,$9,$9,$10,$10,$11)
         ON CONFLICT (document_no) DO NOTHING`,
        [Number(document.rows[0].id), draft.customer_id, Number(draft.branch_id), documentNo, (draft.invoice_issued_at ?? new Date().toISOString()).slice(0, 10), draft.due_date ?? (draft.invoice_issued_at ?? new Date().toISOString()).slice(0, 10), draft.currency.trim(), fx, Number(draft.total_amount), totalThb, officialId],
      );
    } else if (!officialId && step === 'sales_settlement') {
      const ar = await q<{ id: string; document_no: string; open_foreign: string; open_thb: string; currency_code: string }>(
        `SELECT id::text, document_no, open_foreign::text, open_thb::text, currency_code
           FROM finance.ar_documents
          WHERE customer_id = $1 AND status IN ('open','partially_paid')
            AND document_no = coalesce($2, document_no)
          ORDER BY id DESC LIMIT 1 FOR UPDATE`,
        [draft.customer_id, draft.invoice_number],
      );
      if (!ar.rows[0]) throw new Error('Open AR invoice not found for settlement');
      const open = Number(ar.rows[0].open_thb);
      const official = await postJournalInTransaction(q as FinanceQuery, {
        postingDate: new Date().toISOString().slice(0, 10),
        description: `Customer receipt ${ar.rows[0].document_no}`,
        sourceType: 'ar_receipt',
        sourceId: String(draft.so_id),
        sourceEventKey: `sales:${draft.so_id}:sales_settlement:v1`,
        branchId: Number(draft.branch_id),
        lines: [
          { accountCode: '110200', description: `Receipt ${ar.rows[0].document_no}`, debitThb: open, branchId: Number(draft.branch_id), customerId: draft.customer_id },
          { accountCode: '110400', description: `Clear AR ${ar.rows[0].document_no}`, creditThb: open, foreignAmount: Number(ar.rows[0].open_foreign), currencyCode: ar.rows[0].currency_code, branchId: Number(draft.branch_id), customerId: draft.customer_id },
        ],
      }, actor);
      officialId = official.id;
      await q(
        `INSERT INTO finance.ar_allocations
           (ar_document_id, allocation_date, foreign_amount, functional_amount, journal_id, allocated_by)
         VALUES ($1,current_date,$2,$3,$4,$5)`,
        [Number(ar.rows[0].id), Number(ar.rows[0].open_foreign), open, officialId, args.actorId],
      );
      await q(`UPDATE finance.ar_documents SET open_foreign = 0, open_thb = 0, status = 'paid' WHERE id = $1`, [Number(ar.rows[0].id)]);
    }
    await q(
      `UPDATE journal_entries
          SET is_draft = FALSE,
              finalized_at = now(),
              finalized_by = $1,
              approved_by = $1
        WHERE id = $2`,
      [args.actorId, draft.id],
    );
    return { journalId: officialId || draft.id };
  });
}

export async function loadDraftSalesJournal(args: {
  salesOrderId: number;
  step: SalesStep;
}): Promise<{ journalId: number } | null> {
  const step = args.step === 'sales_vat' ? 'sales_accrual' : args.step;
  const r = await query<{ id: number }>(
    `SELECT id FROM journal_entries
      WHERE so_id = $1 AND step = $2 AND is_draft = TRUE
      LIMIT 1`,
    [args.salesOrderId, step],
  );
  return r.rows[0] ? { journalId: r.rows[0].id } : null;
}
