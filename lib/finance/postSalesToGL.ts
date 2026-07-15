import 'server-only';
import { query, withTransaction } from '../db';

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
}

interface SalesGlConfig {
  vatAccountCode: string;
  arAccountCode: string;
  cashAccountCode: string;
  revenueAccountCode: string;
}

const T = {
  vatAccrual: {
    en: (id: number) => `SO-${id} VAT accrual`,
    th: (id: number) => `SO-${id} ตั้งภาษีซื้อ`,
    de: (id: number) => `SO-${id} USt-Rückstellung`,
  },
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
  outputVatReceivable: {
    en: 'Output VAT Receivable',
    th: 'ภาษีซื้อรอเรียกเก็บ',
    de: 'Forderung USt',
  },
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

async function buildVatLines(
  salesOrderId: number,
  cfg: SalesGlConfig,
  locale: Locale,
): Promise<SalesGlLine[]> {
  const totals = await loadTotals(salesOrderId);
  const vat = totals.vat;
  if (vat <= 0) return [];
  const coaCodes = new Set([cfg.vatAccountCode]);
  const arCodes = await loadCoaNames([cfg.vatAccountCode, '120100']);
  void arCodes;
  const coaMap = await loadCoaNames(Array.from(coaCodes));
  const acct = coaMap.get(cfg.vatAccountCode);
  const descAccrual =
    locale === 'de'
      ? T.vatAccrual.de(salesOrderId)
      : locale === 'th'
        ? T.vatAccrual.th(salesOrderId)
        : T.vatAccrual.en(salesOrderId);
  const descPayable =
    locale === 'de'
      ? T.vatPayable.de(salesOrderId)
      : locale === 'th'
        ? T.vatPayable.th(salesOrderId)
        : T.vatPayable.en(salesOrderId);
  const nameRecv =
    locale === 'de'
      ? ACCOUNT_FALLBACK.outputVatReceivable.de
      : locale === 'th'
        ? ACCOUNT_FALLBACK.outputVatReceivable.th
        : ACCOUNT_FALLBACK.outputVatReceivable.en;
  const namePayable =
    locale === 'de'
      ? ACCOUNT_FALLBACK.outputVatPayable.de
      : locale === 'th'
        ? ACCOUNT_FALLBACK.outputVatPayable.th
        : ACCOUNT_FALLBACK.outputVatPayable.en;
  return [
    {
      account_code: '120100',
      account_name: ACCOUNT_FALLBACK.outputVatReceivable.en,
      account_name_th: nameRecv,
      debit: vat,
      credit: 0,
      description: descAccrual,
    },
    {
      account_code: cfg.vatAccountCode,
      account_name: acct?.name ?? ACCOUNT_FALLBACK.outputVatPayable.en,
      account_name_th: namePayable,
      debit: 0,
      credit: vat,
      description: descPayable,
    },
  ];
}

async function buildAccrualLines(
  salesOrderId: number,
  cfg: SalesGlConfig,
  locale: Locale,
): Promise<SalesGlLine[]> {
  const items = await query<{
    line_total: string;
    description: string;
    mapped_revenue_account_code: string | null;
    account_name: string | null;
    account_name_th: string | null;
  }>(
    `SELECT i.line_total::text,
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

  const coaCodes = new Set<string>([cfg.arAccountCode]);
  for (const it of items.rows) {
    if (it.mapped_revenue_account_code) coaCodes.add(it.mapped_revenue_account_code);
    else coaCodes.add(cfg.revenueAccountCode);
  }
  const coaMap = await loadCoaNames(Array.from(coaCodes));
  const acct = coaMap.get(cfg.arAccountCode);

  const revenueByAccount = new Map<string, { amount: number; description: string | null; accountName: string | null; accountNameTh: string | null }>();
  for (const it of items.rows) {
    const amt = parseFloat(it.line_total);
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

  return lines;
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
      const desc =
        locale === 'de'
          ? T.draftDesc.de(vendorName, step, salesOrderId)
          : locale === 'th'
            ? T.draftDesc.th(vendorName, step, salesOrderId)
            : T.draftDesc.en(vendorName, step, salesOrderId);
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
  return { journalId, lines };
}

export async function upsertSalesDraftVat(args: {
  salesOrderId: number;
  vendorName: string;
  locale?: Locale;
}): Promise<SalesGlResult> {
  const locale: Locale = args.locale ?? 'th';
  const cfg = await loadConfig(args.salesOrderId);
  return upsertDraftStep(
    args.salesOrderId,
    'sales_vat',
    () => buildVatLines(args.salesOrderId, cfg, locale),
    args.vendorName,
    locale,
  );
}

export async function upsertSalesDraftAccrual(args: {
  salesOrderId: number;
  vendorName: string;
  locale?: Locale;
}): Promise<SalesGlResult> {
  const locale: Locale = args.locale ?? 'th';
  const cfg = await loadConfig(args.salesOrderId);
  return upsertDraftStep(
    args.salesOrderId,
    'sales_accrual',
    () => buildAccrualLines(args.salesOrderId, cfg, locale),
    args.vendorName,
    locale,
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
    const r = await q<{ id: number }>(
      `SELECT id FROM journal_entries
        WHERE id = $1 AND is_draft = TRUE AND draft_source = 'so'`,
      [args.journalId],
    );
    const draft = r.rows[0];
    if (!draft) return null;
    await q(
      `UPDATE journal_entries
          SET is_draft = FALSE,
              finalized_at = now(),
              finalized_by = $1
        WHERE id = $2`,
      [args.actorId, draft.id],
    );
    return { journalId: draft.id };
  });
}

export async function loadDraftSalesJournal(args: {
  salesOrderId: number;
  step: SalesStep;
}): Promise<{ journalId: number } | null> {
  const r = await query<{ id: number }>(
    `SELECT id FROM journal_entries
      WHERE so_id = $1 AND step = $2 AND is_draft = TRUE
      LIMIT 1`,
    [args.salesOrderId, args.step],
  );
  return r.rows[0] ? { journalId: r.rows[0].id } : null;
}
