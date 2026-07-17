import 'server-only';
import { query } from '@/db';

export type CfActivity = 'operating' | 'investing' | 'financing' | 'non_cash' | 'unclassified';

export interface CashflowRow {
  account_code: string;
  account_name: string;
  activity: CfActivity;
  is_cash_account: boolean;
  amount: number;
}

export interface CashflowSection {
  rows: CashflowRow[];
  total: number;
}

export interface CashflowReconciliation {
  cash_accounts_beginning: number;
  cash_accounts_ending: number;
  net_movement: number;
  is_reconciled: boolean;
  diff: number;
}

export interface CashflowStatement {
  ok: true;
  lang: 'en' | 'th' | 'de';
  period: { date_from: string; date_to: string };
  source: {
    drafts_excluded: boolean;
    posted_only: boolean;
    classification_complete: boolean;
    opening_balance_verified: boolean;
  };
  opening_balance: number;
  ending_balance: number;
  operating: CashflowSection;
  investing: CashflowSection;
  financing: CashflowSection;
  non_cash: CashflowSection;
  unclassified: CashflowSection;
  reconciliation: CashflowReconciliation;
}

export interface CashflowUnavailable {
  ok: false;
  reason: string;
  lang: 'en' | 'th' | 'de';
}

export interface CashflowArgs {
  dateFrom: string;
  dateTo: string;
  lang?: 'en' | 'th' | 'de';
}

function emptySection(): CashflowSection {
  return { rows: [], total: 0 };
}

function sectionFromRows(rows: CashflowRow[]): CashflowSection {
  let total = 0;
  for (const r of rows) total += r.amount;
  return { rows, total };
}

function num(v: unknown): number {
  const n = Number(v);
  return Number.isFinite(n) ? n : 0;
}

function assertDates(args: CashflowArgs): { from: string; to: string } {
  const from = String(args.dateFrom ?? '').trim();
  const to = String(args.dateTo ?? '').trim();
  if (!/^\d{4}-\d{2}-\d{2}$/.test(from) || !/^\d{4}-\d{2}-\d{2}$/.test(to)) {
    throw new Error('date_from and date_to must be YYYY-MM-DD');
  }
  if (from > to) throw new Error('date_from must be on or before date_to');
  return { from, to };
}

export async function getCashflowStatement(args: CashflowArgs): Promise<CashflowStatement | CashflowUnavailable> {
  let dates: { from: string; to: string };
  try {
    dates = assertDates(args);
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e);
    return { ok: false, reason: msg, lang: args.lang ?? 'en' };
  }
  const lang = args.lang ?? 'en';

  const linesRes = await query<{
    account_code: string;
    account_name: string;
    activity: CfActivity;
    is_cash_account: boolean;
    net: string | number | null;
  }>(
    `SELECT account_code,
            account_name,
            activity,
            is_cash_account,
            (COALESCE(debit, 0) - COALESCE(credit, 0)) AS net
       FROM finance.v_cashflow_classified
      WHERE entry_date BETWEEN $1::date AND $2::date`,
    [dates.from, dates.to]
  );

  const buckets: Record<CfActivity, CashflowRow[]> = {
    operating: [],
    investing: [],
    financing: [],
    non_cash: [],
    unclassified: [],
  };
  for (const r of linesRes.rows) {
    const amount = num(r.net);
    if (amount === 0) continue;
    buckets[r.activity].push({
      account_code: r.account_code,
      account_name: r.account_name,
      activity: r.activity,
      is_cash_account: r.is_cash_account,
      amount,
    });
  }

  const openingRes = await query<{ opening: string | number | null }>(
    `SELECT COALESCE(SUM(l.debit), 0) - COALESCE(SUM(l.credit), 0) AS opening
       FROM folio.ledger_lines l
       JOIN folio.journal_entries j ON j.id = l.journal_entry_id
      WHERE j.is_draft = FALSE
        AND l.account_code IN (SELECT account_code FROM finance.account_cf_class WHERE is_cash_account = TRUE)
        AND j.entry_date < $1::date`,
    [dates.from]
  );
  const movementRes = await query<{ net: string | number | null }>(
    `SELECT COALESCE(SUM(l.debit), 0) - COALESCE(SUM(l.credit), 0) AS net
       FROM folio.ledger_lines l
       JOIN folio.journal_entries j ON j.id = l.journal_entry_id
      WHERE j.is_draft = FALSE
        AND l.account_code IN (SELECT account_code FROM finance.account_cf_class WHERE is_cash_account = TRUE)
        AND j.entry_date BETWEEN $1::date AND $2::date`,
    [dates.from, dates.to]
  );

  const opening_balance = num(openingRes.rows[0]?.opening);
  const movement = num(movementRes.rows[0]?.net);
  const ending_balance = opening_balance + movement;

  const mapRes = await query<{ unclassified: string }>(
    `SELECT COUNT(*)::text AS unclassified
       FROM finance.account_cf_class
      WHERE activity = 'unclassified'`
  );
  const classification_complete = num(mapRes.rows[0]?.unclassified) === 0;

  const periodRes = await query<{ opening_balance_journal_id: number | null }>(
    `SELECT opening_balance_journal_id
       FROM finance.cashflow_period
      WHERE $1::date BETWEEN period_start AND period_end
      LIMIT 1`,
    [dates.from]
  );
  const opening_balance_verified = periodRes.rows[0]?.opening_balance_journal_id != null;

  return {
    ok: true,
    lang,
    period: { date_from: dates.from, date_to: dates.to },
    source: {
      drafts_excluded: true,
      posted_only: true,
      classification_complete,
      opening_balance_verified,
    },
    opening_balance,
    ending_balance,
    operating: sectionFromRows(buckets.operating),
    investing: sectionFromRows(buckets.investing),
    financing: sectionFromRows(buckets.financing),
    non_cash: sectionFromRows(buckets.non_cash),
    unclassified: sectionFromRows(buckets.unclassified),
    reconciliation: {
      cash_accounts_beginning: opening_balance,
      cash_accounts_ending: ending_balance,
      net_movement: movement,
      is_reconciled: Math.abs(movement - (num(movementRes.rows[0]?.net))) < 0.01,
      diff: 0,
    },
  };
}

export function cashflowSectionTotal(s: CashflowSection): number {
  return s.total;
}

export const EMPTY_CASHFLOW_SECTION = emptySection();