import 'server-only';
import { query } from '@/db';
import { getCashflowStatement } from './cashflow';
import type { CashflowStatement, CashflowUnavailable } from './cashflow';
import { formatMoneyServer } from '@/components/i18n/formattersServer';
import type { SecondaryLocale } from '@/components/i18n';

export type ReportIntent =
  | 'cash_flow'
  | 'trial_balance'
  | 'income_statement'
  | 'balance_sheet'
  | 'period_summary';

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
}

function num(v: unknown): number {
  const n = Number(v);
  return Number.isFinite(n) ? n : 0;
}

function fmt(n: number, locale: SecondaryLocale): Promise<string> {
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
      const cf = await getCashflowStatement({ dateFrom: dates.from, dateTo: dates.to, lang });
      if (!cf.ok) return { ok: false, intent: 'cash_flow', reason: cf.reason, lang };
      return cashflowToReport(cf, lang, dates);
    }
    case 'trial_balance': return resolveTrialBalance(dates, lang);
    case 'income_statement': return resolveIncomeStatement(dates, lang);
    case 'balance_sheet': return resolveBalanceSheet(dates, lang);
    case 'period_summary': return resolvePeriodSummary(dates, lang);
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

async function resolveTrialBalance(dates: { from: string; to: string }, lang: 'en' | 'th' | 'de'): Promise<ReportResult> {
  const th = isThai(lang);
  const locale = localeOf(lang);
  const r = await query<{ code: string; name: string; account_type: string; period_debit: string | number; period_credit: string | number; net: string | number }>(
    `SELECT code, name, account_type, period_debit, period_credit, net
       FROM finance.v_trial_balance
      ORDER BY code`,
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

async function resolveIncomeStatement(dates: { from: string; to: string }, lang: 'en' | 'th' | 'de'): Promise<ReportResult> {
  const th = isThai(lang);
  const locale = localeOf(lang);
  const r = await query<{ code: string; name: string; name_th: string; account_type: string; amount: string | number }>(
    `SELECT code, name, name_th, account_type, amount
       FROM finance.v_income_statement
      ORDER BY account_type DESC, code`,
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

async function resolveBalanceSheet(dates: { from: string; to: string }, lang: 'en' | 'th' | 'de'): Promise<ReportResult> {
  const th = isThai(lang);
  const locale = localeOf(lang);
  const r = await query<{ code: string; name: string; name_th: string; account_type: string; balance: string | number }>(
    `SELECT code, name, name_th, account_type, balance
       FROM finance.v_balance_sheet
      ORDER BY account_type, code`,
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

async function resolvePeriodSummary(dates: { from: string; to: string }, lang: 'en' | 'th' | 'de'): Promise<ReportResult> {
  const th = isThai(lang);
  const locale = localeOf(lang);
  const r = await query<{ journal_entry_id: number; entry_date: string; description: string; total_debit: string | number; total_credit: string | number }>(
    `SELECT journal_entry_id, entry_date, description, total_debit, total_credit
       FROM finance.v_period_summary
      WHERE entry_date BETWEEN $1::date AND $2::date
      ORDER BY entry_date DESC, journal_entry_id DESC
      LIMIT 100`,
    [dates.from, dates.to],
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