import 'server-only';
import { query } from '@/db';
import { getCashflowStatement } from './cashflow';
import type { CashflowStatement } from './cashflow';
import { formatMoneyServer } from '@/components/i18n/formattersServer';
import type { SecondaryLocale } from '@/components/i18n';

export type ReportIntent =
  | 'cash_flow'
  | 'trial_balance'
  | 'income_statement'
  | 'balance_sheet'
  | 'period_summary'
  | 'ar_aging'
  | 'ap_aging'
  | 'fx_exposure'
  | 'inventory_valuation'
  | 'gross_margin'
  | 'vat_register'
  | 'wht_register'
  | 'budget_vs_actual';

export interface ReportKpi {
  label: string;
  labelTh: string;
  value: string;
  tone?: 'positive' | 'negative' | 'neutral';
  hint?: string;
}

export interface ReportSection {
  title: string;
  titleTh: string;
  rows: Array<Array<string | number>>;
  columns: string[];
  totalLabel?: string;
  total?: number;
}

export interface ReportResult {
  ok: true;
  intent: ReportIntent;
  lang: 'en' | 'th' | 'de';
  title: string;
  titleTh: string;
  subtitle: string;
  subtitleTh: string;
  period: { date_from: string; date_to: string };
  kpis: ReportKpi[];
  sections: ReportSection[];
  notes: string[];
  source: {
    posted_only: boolean;
    drafts_excluded: boolean;
    classification_complete?: boolean;
    opening_balance_verified?: boolean;
  };
}

export interface ReportUnavailable {
  ok: false;
  intent: ReportIntent;
  reason: string;
  lang: 'en' | 'th' | 'de';
}

export interface ResolveReportArgs {
  intent: ReportIntent;
  dateFrom: string;
  dateTo: string;
  lang?: 'en' | 'th' | 'de';
  topN?: number;
  branchId?: number | null;
}

function num(v: unknown): number {
  const n = Number(v);
  return Number.isFinite(n) ? n : 0;
}

function fmt(n: number, locale: SecondaryLocale): string {
  return formatMoneyServer(n, locale);
}

function localeOf(lang: 'en' | 'th' | 'de'): SecondaryLocale {
  return lang === 'de' ? 'de' : 'th';
}

function assertDates(args: ResolveReportArgs): { from: string; to: string } {
  const from = String(args.dateFrom ?? '').trim();
  const to = String(args.dateTo ?? '').trim();
  if (!/^\d{4}-\d{2}-\d{2}$/.test(from) || !/^\d{4}-\d{2}-\d{2}$/.test(to)) {
    throw new Error('date_from and date_to must be YYYY-MM-DD');
  }
  if (from > to) throw new Error('date_from must be on or before date_to');
  return { from, to };
}

function isThai(lang: string | undefined): boolean {
  return lang === 'th';
}

export async function resolveReport(args: ResolveReportArgs): Promise<ReportResult | ReportUnavailable> {
  const lang = args.lang ?? 'en';
  let dates: { from: string; to: string };
  try {
    dates = assertDates(args);
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e);
    return { ok: false, intent: args.intent, reason: msg, lang };
  }

  switch (args.intent) {
    case 'cash_flow': {
      const cf = await getCashflowStatement({ dateFrom: dates.from, dateTo: dates.to, lang, branchId: args.branchId });
      if (!cf.ok) return { ok: false, intent: 'cash_flow', reason: cf.reason, lang };
      return cashflowToReport(cf, lang, dates);
    }
    case 'trial_balance': return resolveTrialBalance(dates, lang, args.branchId);
    case 'income_statement': return resolveIncomeStatement(dates, lang, args.branchId);
    case 'balance_sheet': return resolveBalanceSheet(dates, lang, args.branchId);
    case 'period_summary': return resolvePeriodSummary(dates, lang, args.branchId);
    case 'ar_aging': return resolveAging('ar', dates, lang, args.branchId);
    case 'ap_aging': return resolveAging('ap', dates, lang, args.branchId);
    case 'fx_exposure': return resolveFxExposure(dates, lang, args.branchId);
    case 'inventory_valuation': return resolveInventoryValuation(dates, lang, args.branchId);
    case 'gross_margin': return resolveGrossMargin(dates, lang, args.branchId);
    case 'vat_register': return resolveTaxRegister('vat', dates, lang, args.branchId);
    case 'wht_register': return resolveTaxRegister('wht', dates, lang, args.branchId);
    case 'budget_vs_actual': return resolveBudgetVsActual(dates, lang, args.branchId);
  }
}

async function sectionFromRows(locale: SecondaryLocale, title: string, titleTh: string, columns: string[], rows: Array<{ code: string; name: string; amount: number; side: string }>, total?: number, totalLabel = 'Total'): Promise<ReportSection> {
  const data: Array<Array<string | number>> = await Promise.all(rows.map(async r => [r.code, r.name, await fmt(r.amount, locale), r.side]));
  return { title, titleTh, columns, rows: data, total, totalLabel };
}

async function cashflowToReport(cf: CashflowStatement, lang: 'en' | 'th' | 'de', dates: { from: string; to: string }): Promise<ReportResult> {
  const th = isThai(lang);
  const locale = localeOf(lang);
  const netMovement = cf.operating.total + cf.investing.total + cf.financing.total;
  const sections: ReportSection[] = [];
  const mk = async (titleEn: string, titleTh: string, src: { rows: { account_code: string; account_name: string; amount: number }[]; total: number }) => sectionFromRows(
    locale,
    titleEn, titleTh,
    th ? ['รหัส', 'บัญชี', 'จำนวน', 'ทิศทาง'] : ['Code', 'Account', 'Amount', 'Direction'],
    src.rows.map(r => ({ code: r.account_code, name: r.account_name, amount: r.amount, side: r.amount >= 0 ? (th ? 'เข้า' : 'in') : (th ? 'ออก' : 'out') })),
    src.total,
    th ? 'รวม' : 'Total',
  );
  sections.push(await mk('Operating', 'กิจกรรมดำเนินงาน', { rows: cf.operating.rows, total: cf.operating.total }));
  sections.push(await mk('Investing', 'กิจกรรมลงทุน', { rows: cf.investing.rows, total: cf.investing.total }));
  sections.push(await mk('Financing', 'กิจกรรมจัดหาเงิน', { rows: cf.financing.rows, total: cf.financing.total }));
  if (cf.non_cash.rows.length > 0) {
    sections.push(await mk('Non-cash', 'ไม่เป็นเงินสด', { rows: cf.non_cash.rows, total: cf.non_cash.total }));
  }
  if (cf.unclassified.rows.length > 0) {
    sections.push({
      title: 'Unclassified', titleTh: 'ยังไม่จัดประเภท',
      columns: th ? ['รหัส', 'บัญชี', 'จำนวน'] : ['Code', 'Account', 'Amount'],
      rows: await Promise.all(cf.unclassified.rows.map(async r => [r.account_code, r.account_name, await fmt(r.amount, locale)])),
      total: cf.unclassified.total,
      totalLabel: th ? 'รวม' : 'Total',
    });
  }
  const notes: string[] = [];
  if (!cf.source.classification_complete) notes.push(th
    ? 'ยังมีบัญชีที่ไม่ได้จัดประเภท — กรุณาเพิ่มใน finance.account_cf_class'
    : 'Some accounts are unclassified — add them to finance.account_cf_class.');
  if (!cf.source.opening_balance_verified) notes.push(th
    ? 'ยอดยกมายังไม่ได้รับการยืนยัน — ลงทะเบียนงวดใน finance.cashflow_period'
    : 'Opening balance is not verified — register the period in finance.cashflow_period.');

  return {
    ok: true,
    intent: 'cash_flow',
    lang,
    title: 'Cash Flow Statement',
    titleTh: 'งบกระแสเงินสด',
    subtitle: `${dates.from} → ${dates.to}`,
    subtitleTh: `ช่วง ${dates.from} ถึง ${dates.to}`,
    period: { date_from: dates.from, date_to: dates.to },
    kpis: await Promise.all([
      { label: 'Opening cash', labelTh: 'เงินสดต้นงวด', value: await fmt(cf.opening_balance, locale), tone: 'neutral' as const },
      { label: 'Net movement', labelTh: 'กระแสเงินสดสุทธิ', value: await fmt(netMovement, locale), tone: netMovement >= 0 ? ('positive' as const) : ('negative' as const) },
      { label: 'Ending cash', labelTh: 'เงินสดปลายงวด', value: await fmt(cf.ending_balance, locale), tone: 'neutral' as const },
    ]),
    sections,
    notes,
    source: {
      posted_only: cf.source.posted_only,
      drafts_excluded: cf.source.drafts_excluded,
      classification_complete: cf.source.classification_complete,
      opening_balance_verified: cf.source.opening_balance_verified,
    },
  };
}

async function resolveTrialBalance(dates: { from: string; to: string }, lang: 'en' | 'th' | 'de', branchId?: number | null): Promise<ReportResult> {
  const th = isThai(lang);
  const locale = localeOf(lang);
  const r = await query<{ code: string; name: string; account_type: string; period_debit: string | number; period_credit: string | number; net: string | number }>(
    `SELECT code, name, account_type, debit AS period_debit, credit AS period_credit,
            debit - credit AS net
       FROM finance.trial_balance($1::date, $2::date, $3::bigint)
      ORDER BY code`,
    [dates.from, dates.to, branchId ?? null],
  );
  const rows = r.rows.map(rr => ({ code: rr.code, name: th ? rr.name : rr.name, amount: num(rr.period_debit), side: 'Dr' as string }));
  const creditRows = r.rows.map(rr => ({ code: rr.code, name: rr.name, amount: num(rr.period_credit), side: 'Cr' as string }));
  const totalDebit = rows.reduce((s, x) => s + x.amount, 0);
  const totalCredit = creditRows.reduce((s, x) => s + x.amount, 0);
  const combined = r.rows.map(rr => ({
    code: rr.code,
    name: rr.name,
    debit: num(rr.period_debit),
    credit: num(rr.period_credit),
    net: num(rr.net),
  }));
  return {
    ok: true,
    intent: 'trial_balance',
    lang,
    title: 'Trial Balance',
    titleTh: 'งบทดลอง',
    subtitle: `${dates.from} → ${dates.to}`,
    subtitleTh: `ช่วง ${dates.from} ถึง ${dates.to}`,
    period: { date_from: dates.from, date_to: dates.to },
    kpis: [
      { label: 'Total debits', labelTh: 'รวมเดบิต', value: await fmt(totalDebit, locale), tone: 'neutral' },
      { label: 'Total credits', labelTh: 'รวมเครดิต', value: await fmt(totalCredit, locale), tone: 'neutral' },
      { label: 'Variance', labelTh: 'ส่วนต่าง', value: await fmt(Math.abs(totalDebit - totalCredit), locale), tone: Math.abs(totalDebit - totalCredit) < 0.01 ? 'positive' : 'negative', hint: Math.abs(totalDebit - totalCredit) < 0.01 ? (th ? 'สมดุล' : 'Balanced') : (th ? 'ไม่สมดุล' : 'Out of balance') },
    ],
    sections: [
      {
        title: 'Accounts', titleTh: 'บัญชี',
        columns: th ? ['รหัส', 'ชื่อ', 'เดบิต', 'เครดิต', 'คงเหลือ'] : ['Code', 'Name', 'Debit', 'Credit', 'Net'],
        rows: await Promise.all(combined.map(async c => [c.code, c.name, await fmt(c.debit, locale), await fmt(c.credit, locale), await fmt(c.net, locale)])),
        total: totalDebit - totalCredit,
        totalLabel: th ? 'รวมสุทธิ' : 'Net total',
      },
    ],
    notes: Math.abs(totalDebit - totalCredit) < 0.01
      ? []
      : [th ? 'งบทดลองไม่สมดุล — ตรวจสอบรายการที่ยังไม่ได้ลงบัญชี' : 'Trial balance is not balanced — check unposted lines.'],
    source: { posted_only: true, drafts_excluded: true },
  };
}

async function resolveIncomeStatement(dates: { from: string; to: string }, lang: 'en' | 'th' | 'de', branchId?: number | null): Promise<ReportResult> {
  const th = isThai(lang);
  const locale = localeOf(lang);
  const r = await query<{ code: string; name: string; name_th: string; account_type: string; amount: string | number }>(
    `SELECT p.code, p.name, a.name_th, p.account_type, p.amount
       FROM finance.profit_and_loss($1::date, $2::date, $3::bigint) p
       JOIN finance.accounts a ON a.code = p.code
      ORDER BY p.account_type DESC, p.code`,
    [dates.from, dates.to, branchId ?? null],
  );
  const revenue = r.rows.filter(rr => rr.account_type === 'revenue');
  const expense = r.rows.filter(rr => rr.account_type === 'expense');
  const totalRevenue = revenue.reduce((s, x) => s + num(x.amount), 0);
  const totalExpense = expense.reduce((s, x) => s + num(x.amount), 0);
  const netIncome = totalRevenue - totalExpense;
  return {
    ok: true,
    intent: 'income_statement',
    lang,
    title: 'Income Statement',
    titleTh: 'งบกำไรขาดทุน',
    subtitle: `${dates.from} → ${dates.to}`,
    subtitleTh: `ช่วง ${dates.from} ถึง ${dates.to}`,
    period: { date_from: dates.from, date_to: dates.to },
    kpis: [
      { label: 'Total revenue', labelTh: 'รายได้รวม', value: await fmt(totalRevenue, locale), tone: 'positive' },
      { label: 'Total expense', labelTh: 'ค่าใช้จ่ายรวม', value: await fmt(totalExpense, locale), tone: 'negative' },
      { label: 'Net income', labelTh: 'กำไรสุทธิ', value: await fmt(netIncome, locale), tone: netIncome >= 0 ? 'positive' : 'negative' },
    ],
    sections: [
      {
        title: 'Revenue', titleTh: 'รายได้',
        columns: th ? ['รหัส', 'บัญชี', 'จำนวน'] : ['Code', 'Account', 'Amount'],
        rows: await Promise.all(revenue.map(async x => [x.code, th ? x.name_th : x.name, await fmt(num(x.amount), locale)])),
        total: totalRevenue,
        totalLabel: th ? 'รวมรายได้' : 'Total revenue',
      },
      {
        title: 'Expenses', titleTh: 'ค่าใช้จ่าย',
        columns: th ? ['รหัส', 'บัญชี', 'จำนวน'] : ['Code', 'Account', 'Amount'],
        rows: await Promise.all(expense.map(async x => [x.code, th ? x.name_th : x.name, await fmt(num(x.amount), locale)])),
        total: totalExpense,
        totalLabel: th ? 'รวมค่าใช้จ่าย' : 'Total expenses',
      },
    ],
    notes: [],
    source: { posted_only: true, drafts_excluded: true },
  };
}

async function resolveBalanceSheet(dates: { from: string; to: string }, lang: 'en' | 'th' | 'de', branchId?: number | null): Promise<ReportResult> {
  const th = isThai(lang);
  const locale = localeOf(lang);
  const r = await query<{ code: string; name: string; name_th: string; account_type: string; balance: string | number }>(
    `SELECT b.code, b.name, a.name_th, b.account_type, b.amount AS balance
       FROM finance.balance_sheet($1::date, $2::bigint) b
       JOIN finance.accounts a ON a.code = b.code
      UNION ALL
     SELECT 'CURRENT_EARNINGS', 'Current earnings', 'กำไรสะสมงวดปัจจุบัน', 'equity',
            coalesce(sum(CASE WHEN account_type = 'revenue' THEN credit_thb - debit_thb
                              WHEN account_type = 'expense' THEN credit_thb - debit_thb ELSE 0 END), 0)
       FROM finance.v_posted_lines
      WHERE posting_date <= $1::date
        AND ($2::bigint IS NULL OR branch_id = $2)
      ORDER BY 4, 1`,
    [dates.to, branchId ?? null],
  );
  const assets = r.rows.filter(x => x.account_type === 'asset');
  const liabilities = r.rows.filter(x => x.account_type === 'liability');
  const equity = r.rows.filter(x => x.account_type === 'equity');
  const totalAssets = assets.reduce((s, x) => s + num(x.balance), 0);
  const totalLiabilities = liabilities.reduce((s, x) => s + num(x.balance), 0);
  const totalEquity = equity.reduce((s, x) => s + num(x.balance), 0);
  return {
    ok: true,
    intent: 'balance_sheet',
    lang,
    title: 'Balance Sheet',
    titleTh: 'งบดุล',
    subtitle: `${dates.from} → ${dates.to}`,
    subtitleTh: `ช่วง ${dates.from} ถึง ${dates.to}`,
    period: { date_from: dates.from, date_to: dates.to },
    kpis: [
      { label: 'Total assets', labelTh: 'สินทรัพย์รวม', value: await fmt(totalAssets, locale), tone: 'positive' },
      { label: 'Total liabilities', labelTh: 'หนี้สินรวม', value: await fmt(totalLiabilities, locale), tone: 'negative' },
      { label: 'Total equity', labelTh: 'ส่วนของผู้ถือหุ้น', value: await fmt(totalEquity, locale), tone: 'neutral' },
    ],
    sections: [
      {
        title: 'Assets', titleTh: 'สินทรัพย์',
        columns: th ? ['รหัส', 'บัญชี', 'คงเหลือ'] : ['Code', 'Account', 'Balance'],
        rows: await Promise.all(assets.map(async x => [x.code, th ? x.name_th : x.name, await fmt(num(x.balance), locale)])),
        total: totalAssets,
        totalLabel: th ? 'รวมสินทรัพย์' : 'Total assets',
      },
      {
        title: 'Liabilities', titleTh: 'หนี้สิน',
        columns: th ? ['รหัส', 'บัญชี', 'คงเหลือ'] : ['Code', 'Account', 'Balance'],
        rows: await Promise.all(liabilities.map(async x => [x.code, th ? x.name_th : x.name, await fmt(num(x.balance), locale)])),
        total: totalLiabilities,
        totalLabel: th ? 'รวมหนี้สิน' : 'Total liabilities',
      },
      {
        title: 'Equity', titleTh: 'ส่วนของผู้ถือหุ้น',
        columns: th ? ['รหัส', 'บัญชี', 'คงเหลือ'] : ['Code', 'Account', 'Balance'],
        rows: await Promise.all(equity.map(async x => [x.code, th ? x.name_th : x.name, await fmt(num(x.balance), locale)])),
        total: totalEquity,
        totalLabel: th ? 'รวมส่วนผู้ถือหุ้น' : 'Total equity',
      },
    ],
    notes: [],
    source: { posted_only: true, drafts_excluded: true },
  };
}

async function resolvePeriodSummary(dates: { from: string; to: string }, lang: 'en' | 'th' | 'de', branchId?: number | null): Promise<ReportResult> {
  const th = isThai(lang);
  const locale = localeOf(lang);
  const r = await query<{ journal_entry_id: number; entry_date: string; description: string; total_debit: string | number; total_credit: string | number }>(
    `SELECT journal_id AS journal_entry_id, posting_date::text AS entry_date, description, total_debit, total_credit
       FROM finance.v_period_summary
      WHERE posting_date BETWEEN $1::date AND $2::date
        AND ($3::bigint IS NULL OR branch_id = $3)
      ORDER BY posting_date DESC, journal_id DESC
      LIMIT 100`,
    [dates.from, dates.to, branchId ?? null],
  );
  const totalDebit = r.rows.reduce((s, x) => s + num(x.total_debit), 0);
  const totalCredit = r.rows.reduce((s, x) => s + num(x.total_credit), 0);
  return {
    ok: true,
    intent: 'period_summary',
    lang,
    title: 'Period Journal Summary',
    titleTh: 'สรุปรายการบัญชีในงวด',
    subtitle: `${dates.from} → ${dates.to}`,
    subtitleTh: `ช่วง ${dates.from} ถึง ${dates.to}`,
    period: { date_from: dates.from, date_to: dates.to },
    kpis: [
      { label: 'Journal entries', labelTh: 'จำนวนรายการ', value: r.rows.length.toString(), tone: 'neutral' },
      { label: 'Total debits', labelTh: 'รวมเดบิต', value: await fmt(totalDebit, locale), tone: 'neutral' },
      { label: 'Total credits', labelTh: 'รวมเครดิต', value: await fmt(totalCredit, locale), tone: 'neutral' },
    ],
    sections: [
      {
        title: 'Entries', titleTh: 'รายการ',
        columns: th ? ['วันที่', 'รายละเอียด', 'เดบิต', 'เครดิต'] : ['Date', 'Description', 'Debit', 'Credit'],
        rows: await Promise.all(r.rows.map(async x => [
          x.entry_date,
          x.description,
          await fmt(num(x.total_debit), locale),
          await fmt(num(x.total_credit), locale),
        ])),
        totalLabel: th ? 'รวม' : 'Total',
        total: totalDebit,
      },
    ],
    notes: [],
    source: { posted_only: true, drafts_excluded: true },
  };
}

async function resolveAging(kind: 'ar' | 'ap', dates: { from: string; to: string }, lang: 'en' | 'th' | 'de', branchId?: number | null): Promise<ReportResult> {
  const th = isThai(lang);
  const locale = localeOf(lang);
  const partyJoin = kind === 'ar'
    ? 'LEFT JOIN folio.customers p ON p.id = d.customer_id'
    : 'LEFT JOIN finance.vendors p ON p.id = d.vendor_id';
  const party = kind === 'ar' ? 'coalesce(p.name, d.customer_id::text)' : "coalesce(p.name, 'Employee #' || d.employee_id::text)";
  const r = await query<{ document_no: string; party: string; due_date: string; currency_code: string; open_foreign: string; open_thb: string; bucket: string }>(
    `SELECT d.document_no, ${party} AS party, d.due_date::text, d.currency_code,
            d.open_foreign::text, d.open_thb::text,
            CASE WHEN $1::date <= d.due_date THEN 'current'
                 WHEN $1::date - d.due_date <= 30 THEN '1_30'
                 WHEN $1::date - d.due_date <= 60 THEN '31_60'
                 WHEN $1::date - d.due_date <= 90 THEN '61_90'
                 ELSE 'over_90' END AS bucket
       FROM finance.${kind}_balance_as_of($1::date, $2::bigint) d
       ${partyJoin}
      ORDER BY d.due_date, d.document_no`,
    [dates.to, branchId ?? null],
  );
  const total = r.rows.reduce((sum, row) => sum + num(row.open_thb), 0);
  const overdue = r.rows.filter((row) => row.bucket !== 'current').reduce((sum, row) => sum + num(row.open_thb), 0);
  const intent: ReportIntent = kind === 'ar' ? 'ar_aging' : 'ap_aging';
  return {
    ok: true,
    intent,
    lang,
    title: kind === 'ar' ? 'Accounts Receivable Aging' : 'Accounts Payable Aging',
    titleTh: kind === 'ar' ? 'อายุลูกหนี้การค้า' : 'อายุเจ้าหนี้การค้า',
    subtitle: `Open as of ${dates.to}`,
    subtitleTh: `ยอดคงค้าง ณ ${dates.to}`,
    period: { date_from: dates.from, date_to: dates.to },
    kpis: [
      { label: 'Open balance', labelTh: 'ยอดคงค้าง', value: await fmt(total, locale), tone: 'neutral' },
      { label: 'Overdue', labelTh: 'เกินกำหนด', value: await fmt(overdue, locale), tone: overdue > 0 ? 'negative' : 'positive' },
      { label: 'Open documents', labelTh: 'จำนวนเอกสาร', value: String(r.rows.length), tone: 'neutral' },
    ],
    sections: [{
      title: 'Source documents', titleTh: 'เอกสารต้นทาง',
      columns: th ? ['เอกสาร', 'คู่ค้า', 'ครบกำหนด', 'สกุลเงิน', 'ยอดต่างประเทศ', 'ยอดบาท', 'ช่วงอายุ'] : ['Document', 'Party', 'Due', 'Currency', 'Foreign open', 'THB open', 'Bucket'],
      rows: await Promise.all(r.rows.map(async (row) => [row.document_no, row.party, row.due_date, row.currency_code, row.open_foreign, await fmt(num(row.open_thb), locale), row.bucket])),
      total,
      totalLabel: th ? 'รวมคงค้าง' : 'Total open',
    }],
    notes: [],
    source: { posted_only: true, drafts_excluded: true },
  };
}

async function resolveFxExposure(dates: { from: string; to: string }, lang: 'en' | 'th' | 'de', branchId?: number | null): Promise<ReportResult> {
  const th = isThai(lang);
  const locale = localeOf(lang);
  const r = await query<{ subledger: string; currency_code: string; open_foreign: string; carrying_thb: string }>(
    `SELECT subledger, currency_code,
            sum(open_foreign)::text AS open_foreign,
            sum(open_thb)::text AS carrying_thb
       FROM (
         SELECT 'AR'::text AS subledger, currency_code, open_foreign, open_thb
           FROM finance.ar_balance_as_of($1::date, $2::bigint) WHERE currency_code <> 'THB'
         UNION ALL
         SELECT 'AP', currency_code, open_foreign, open_thb
           FROM finance.ap_balance_as_of($1::date, $2::bigint) WHERE currency_code <> 'THB'
       ) x
      GROUP BY subledger, currency_code
      ORDER BY currency_code, subledger`,
    [dates.to, branchId ?? null],
  );
  const exposure = r.rows.reduce((sum, row) => sum + Math.abs(num(row.carrying_thb)), 0);
  return {
    ok: true, intent: 'fx_exposure', lang,
    title: 'Foreign Currency Exposure', titleTh: 'ความเสี่ยงอัตราแลกเปลี่ยน',
    subtitle: `Open AR/AP as of ${dates.to}`, subtitleTh: `ลูกหนี้และเจ้าหนี้คงค้าง ณ ${dates.to}`,
    period: { date_from: dates.from, date_to: dates.to },
    kpis: [
      { label: 'Gross THB exposure', labelTh: 'มูลค่าความเสี่ยงรวม', value: await fmt(exposure, locale), tone: exposure > 0 ? 'negative' : 'positive' },
      { label: 'Currencies', labelTh: 'จำนวนสกุลเงิน', value: String(new Set(r.rows.map((row) => row.currency_code)).size), tone: 'neutral' },
      { label: 'Positions', labelTh: 'จำนวนสถานะ', value: String(r.rows.length), tone: 'neutral' },
    ],
    sections: [{
      title: 'Open positions', titleTh: 'สถานะคงค้าง',
      columns: th ? ['บัญชีย่อย', 'สกุลเงิน', 'ยอดต่างประเทศ', 'มูลค่าบาท'] : ['Subledger', 'Currency', 'Foreign open', 'Carrying THB'],
      rows: await Promise.all(r.rows.map(async (row) => [row.subledger, row.currency_code, row.open_foreign, await fmt(num(row.carrying_thb), locale)])),
      total: exposure,
      totalLabel: th ? 'ความเสี่ยงรวม' : 'Gross exposure',
    }],
    notes: [],
    source: { posted_only: true, drafts_excluded: true },
  };
}

async function resolveInventoryValuation(dates: { from: string; to: string }, lang: 'en' | 'th' | 'de', branchId?: number | null): Promise<ReportResult> {
  const th = isThai(lang);
  const locale = localeOf(lang);
  const r = await query<{ sku: string; name: string; warehouse: string; lot_no: string | null; expires_on: string | null; quantity: string; value_thb: string; expiry_bucket: string }>(
    `SELECT p.sku, p.name, w.code AS warehouse, l.lot_no, l.expires_on::text,
            v.quantity::text, v.value_thb::text,
            CASE WHEN l.expires_on IS NULL THEN 'not_tracked'
                 WHEN l.expires_on < $1::date THEN 'expired'
                 WHEN l.expires_on <= $1::date + 30 THEN '0_30_days'
                 WHEN l.expires_on <= $1::date + 90 THEN '31_90_days'
                 ELSE 'over_90_days' END AS expiry_bucket
       FROM inventory.valuation_as_of($1::date, $2::bigint) v
       JOIN inventory.products p ON p.id = v.product_id
       JOIN inventory.warehouses w ON w.id = v.warehouse_id
       LEFT JOIN inventory.lots l ON l.id = v.lot_id
      ORDER BY p.sku, w.code, l.expires_on NULLS LAST`,
    [dates.to, branchId ?? null],
  );
  const value = r.rows.reduce((sum, row) => sum + num(row.value_thb), 0);
  const quantity = r.rows.reduce((sum, row) => sum + num(row.quantity), 0);
  const atRisk = r.rows.filter((row) => ['expired','0_30_days','31_90_days'].includes(row.expiry_bucket)).reduce((sum, row) => sum + num(row.value_thb), 0);
  return {
    ok: true, intent: 'inventory_valuation', lang,
    title: 'Inventory Valuation and Expiry Aging', titleTh: 'มูลค่าและอายุสินค้าคงเหลือ',
    subtitle: `Moving weighted-average valuation as of ${dates.to}`, subtitleTh: `ต้นทุนถัวเฉลี่ยถ่วงน้ำหนัก ณ ${dates.to}`,
    period: { date_from: dates.from, date_to: dates.to },
    kpis: [
      { label: 'Stock value', labelTh: 'มูลค่าสินค้า', value: await fmt(value, locale), tone: 'neutral' },
      { label: 'Quantity', labelTh: 'จำนวน', value: quantity.toLocaleString(), tone: 'neutral' },
      { label: 'Expiry risk ≤ 90 days', labelTh: 'เสี่ยงหมดอายุใน 90 วัน', value: await fmt(atRisk, locale), tone: atRisk > 0 ? 'negative' : 'positive' },
    ],
    sections: [{
      title: 'SKU and lot detail', titleTh: 'รายละเอียดสินค้าและล็อต',
      columns: th ? ['SKU', 'สินค้า', 'คลัง', 'ล็อต', 'หมดอายุ', 'จำนวน', 'มูลค่า', 'ช่วงอายุ'] : ['SKU', 'Product', 'Warehouse', 'Lot', 'Expiry', 'Quantity', 'Value THB', 'Expiry bucket'],
      rows: await Promise.all(r.rows.map(async (row) => [row.sku, row.name, row.warehouse, row.lot_no ?? '—', row.expires_on ?? '—', row.quantity, await fmt(num(row.value_thb), locale), row.expiry_bucket])),
      total: value,
      totalLabel: th ? 'มูลค่ารวม' : 'Total value',
    }],
    notes: [],
    source: { posted_only: true, drafts_excluded: true },
  };
}

async function resolveGrossMargin(dates: { from: string; to: string }, lang: 'en' | 'th' | 'de', branchId?: number | null): Promise<ReportResult> {
  const th = isThai(lang);
  const locale = localeOf(lang);
  const r = await query<{ period: string; revenue: string; cogs: string; gross_margin: string }>(
    `SELECT to_char(posting_date, 'YYYY-MM') AS period,
            sum(CASE WHEN account_type = 'revenue' THEN credit_thb - debit_thb ELSE 0 END)::text AS revenue,
            sum(CASE WHEN account_code LIKE '5101%' THEN debit_thb - credit_thb ELSE 0 END)::text AS cogs,
            sum(CASE WHEN account_type = 'revenue' THEN credit_thb - debit_thb
                     WHEN account_code LIKE '5101%' THEN credit_thb - debit_thb ELSE 0 END)::text AS gross_margin
       FROM finance.v_posted_lines
      WHERE posting_date BETWEEN $1::date AND $2::date
        AND ($3::bigint IS NULL OR branch_id = $3)
      GROUP BY 1 ORDER BY 1`,
    [dates.from, dates.to, branchId ?? null],
  );
  const revenue = r.rows.reduce((sum, row) => sum + num(row.revenue), 0);
  const cogs = r.rows.reduce((sum, row) => sum + num(row.cogs), 0);
  const margin = revenue - cogs;
  return {
    ok: true, intent: 'gross_margin', lang,
    title: 'Gross Margin', titleTh: 'กำไรขั้นต้น',
    subtitle: `${dates.from} → ${dates.to}`, subtitleTh: `ช่วง ${dates.from} ถึง ${dates.to}`,
    period: { date_from: dates.from, date_to: dates.to },
    kpis: [
      { label: 'Revenue', labelTh: 'รายได้', value: await fmt(revenue, locale), tone: 'positive' },
      { label: 'COGS', labelTh: 'ต้นทุนขาย', value: await fmt(cogs, locale), tone: 'negative' },
      { label: 'Gross margin', labelTh: 'กำไรขั้นต้น', value: await fmt(margin, locale), tone: margin >= 0 ? 'positive' : 'negative', hint: revenue ? `${(margin / revenue * 100).toFixed(1)}%` : '0.0%' },
    ],
    sections: [{
      title: 'Monthly posted activity', titleTh: 'รายการรายเดือนที่ลงบัญชีแล้ว',
      columns: th ? ['งวด', 'รายได้', 'ต้นทุนขาย', 'กำไรขั้นต้น', 'อัตรากำไร'] : ['Period', 'Revenue', 'COGS', 'Gross margin', 'Margin %'],
      rows: await Promise.all(r.rows.map(async (row) => [row.period, await fmt(num(row.revenue), locale), await fmt(num(row.cogs), locale), await fmt(num(row.gross_margin), locale), num(row.revenue) ? `${(num(row.gross_margin) / num(row.revenue) * 100).toFixed(1)}%` : '0.0%'])),
      total: margin,
      totalLabel: th ? 'กำไรขั้นต้นรวม' : 'Total gross margin',
    }],
    notes: [],
    source: { posted_only: true, drafts_excluded: true },
  };
}

async function resolveTaxRegister(kind: 'vat' | 'wht', dates: { from: string; to: string }, lang: 'en' | 'th' | 'de', branchId?: number | null): Promise<ReportResult> {
  const th = isThai(lang);
  const locale = localeOf(lang);
  const kinds = kind === 'vat' ? ['vat_input', 'vat_output'] : ['wht_receivable', 'wht_payable'];
  const r = await query<{ posting_date: string; journal_no: string; source_type: string; source_id: string; tax_code: string; tax_name: string; tax_kind: string; debit: string; credit: string; amount: string }>(
    `SELECT l.posting_date::text, l.journal_no, l.source_type, l.source_id,
            t.code AS tax_code, t.name AS tax_name, t.kind AS tax_kind,
            l.debit_thb::text AS debit, l.credit_thb::text AS credit,
            CASE WHEN t.kind IN ('vat_input','wht_receivable') THEN l.debit_thb - l.credit_thb
                 ELSE l.credit_thb - l.debit_thb END::text AS amount
       FROM finance.v_posted_lines l
       JOIN finance.tax_codes t ON t.account_code = l.account_code AND t.kind = ANY($4::text[])
      WHERE l.posting_date BETWEEN $1::date AND $2::date
        AND ($3::bigint IS NULL OR l.branch_id = $3)
      ORDER BY l.posting_date, l.journal_id, l.line_no`,
    [dates.from, dates.to, branchId ?? null, kinds],
  );
  const total = r.rows.reduce((sum, row) => sum + num(row.amount), 0);
  const intent: ReportIntent = kind === 'vat' ? 'vat_register' : 'wht_register';
  return {
    ok: true, intent, lang,
    title: kind === 'vat' ? 'VAT Register' : 'Withholding Tax Register',
    titleTh: kind === 'vat' ? 'ทะเบียนภาษีมูลค่าเพิ่ม' : 'ทะเบียนภาษีหัก ณ ที่จ่าย',
    subtitle: `${dates.from} → ${dates.to}`, subtitleTh: `ช่วง ${dates.from} ถึง ${dates.to}`,
    period: { date_from: dates.from, date_to: dates.to },
    kpis: [
      { label: 'Net tax', labelTh: 'ภาษีสุทธิ', value: await fmt(total, locale), tone: 'neutral' },
      { label: 'Posted tax lines', labelTh: 'จำนวนรายการภาษี', value: String(r.rows.length), tone: 'neutral' },
      { label: 'Tax codes used', labelTh: 'รหัสภาษีที่ใช้', value: String(new Set(r.rows.map((row) => row.tax_code)).size), tone: 'neutral' },
    ],
    sections: [{
      title: 'Posted tax lines', titleTh: 'รายการภาษีที่ลงบัญชีแล้ว',
      columns: th ? ['วันที่', 'สมุดรายวัน', 'ต้นทาง', 'รหัสภาษี', 'ประเภท', 'เดบิต', 'เครดิต', 'สุทธิ'] : ['Date', 'Journal', 'Source', 'Tax code', 'Kind', 'Debit', 'Credit', 'Net'],
      rows: await Promise.all(r.rows.map(async (row) => [row.posting_date, row.journal_no, `${row.source_type}:${row.source_id}`, `${row.tax_code} · ${row.tax_name}`, row.tax_kind, await fmt(num(row.debit), locale), await fmt(num(row.credit), locale), await fmt(num(row.amount), locale)])),
      total,
      totalLabel: th ? 'ภาษีสุทธิ' : 'Net tax',
    }],
    notes: [],
    source: { posted_only: true, drafts_excluded: true },
  };
}

async function resolveBudgetVsActual(dates: { from: string; to: string }, lang: 'en' | 'th' | 'de', branchId?: number | null): Promise<ReportResult> {
  const th = isThai(lang);
  const locale = localeOf(lang);
  const r = await query<{ fiscal_year: number; period_no: number; account_code: string; account_name: string; department_id: string | null; budget: string; actual: string; variance: string }>(
    `WITH approved AS (
       SELECT b.fiscal_year, bl.period_no, bl.account_code, a.name AS account_name,
              bl.department_id, sum(bl.amount_thb) AS budget
         FROM finance.budgets b
         JOIN finance.budget_lines bl ON bl.budget_id = b.id
         JOIN finance.accounts a ON a.code = bl.account_code
        WHERE b.status = 'approved'
          AND make_date(b.fiscal_year, bl.period_no, 1) BETWEEN date_trunc('month', $1::date)::date AND date_trunc('month', $2::date)::date
          AND ($3::bigint IS NULL OR b.branch_id IS NULL OR b.branch_id = $3)
        GROUP BY b.fiscal_year, bl.period_no, bl.account_code, a.name, bl.department_id
     )
     SELECT b.fiscal_year, b.period_no, b.account_code, b.account_name, b.department_id,
            b.budget::text,
            coalesce(sum(CASE WHEN a.account_type = 'revenue' THEN l.credit_thb - l.debit_thb ELSE l.debit_thb - l.credit_thb END), 0)::text AS actual,
            (b.budget - coalesce(sum(CASE WHEN a.account_type = 'revenue' THEN l.credit_thb - l.debit_thb ELSE l.debit_thb - l.credit_thb END), 0))::text AS variance
       FROM approved b
       JOIN finance.accounts a ON a.code = b.account_code
       LEFT JOIN finance.v_posted_lines l ON l.account_code = b.account_code
        AND extract(year from l.posting_date) = b.fiscal_year
        AND extract(month from l.posting_date) = b.period_no
        AND ($3::bigint IS NULL OR l.branch_id = $3)
        AND (b.department_id IS NULL OR l.department_id = b.department_id)
      GROUP BY b.fiscal_year, b.period_no, b.account_code, b.account_name, b.department_id, b.budget
      ORDER BY b.fiscal_year, b.period_no, b.account_code, b.department_id NULLS FIRST`,
    [dates.from, dates.to, branchId ?? null],
  );
  const budget = r.rows.reduce((sum, row) => sum + num(row.budget), 0);
  const actual = r.rows.reduce((sum, row) => sum + num(row.actual), 0);
  const variance = budget - actual;
  return {
    ok: true, intent: 'budget_vs_actual', lang,
    title: 'Budget versus Actual', titleTh: 'งบประมาณเทียบผลจริง',
    subtitle: `${dates.from} → ${dates.to}`, subtitleTh: `ช่วง ${dates.from} ถึง ${dates.to}`,
    period: { date_from: dates.from, date_to: dates.to },
    kpis: [
      { label: 'Approved budget', labelTh: 'งบประมาณอนุมัติ', value: await fmt(budget, locale), tone: 'neutral' },
      { label: 'Posted actual', labelTh: 'ผลจริงที่ลงบัญชี', value: await fmt(actual, locale), tone: 'neutral' },
      { label: 'Variance', labelTh: 'ส่วนต่าง', value: await fmt(variance, locale), tone: variance >= 0 ? 'positive' : 'negative' },
    ],
    sections: [{
      title: 'Monthly account detail', titleTh: 'รายละเอียดรายเดือนและบัญชี',
      columns: th ? ['งวด', 'บัญชี', 'แผนก', 'งบประมาณ', 'ผลจริง', 'ส่วนต่าง'] : ['Period', 'Account', 'Department', 'Budget', 'Actual', 'Variance'],
      rows: await Promise.all(r.rows.map(async (row) => [`${row.fiscal_year}-${String(row.period_no).padStart(2, '0')}`, `${row.account_code} · ${row.account_name}`, row.department_id ?? 'All', await fmt(num(row.budget), locale), await fmt(num(row.actual), locale), await fmt(num(row.variance), locale)])),
      total: variance,
      totalLabel: th ? 'ส่วนต่างรวม' : 'Total variance',
    }],
    notes: [],
    source: { posted_only: true, drafts_excluded: true },
  };
}
