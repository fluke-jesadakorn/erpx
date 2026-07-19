import 'server-only';
import { query } from '@/db';
import type { ReportFilter } from './types';

export interface FinanceKpis {
  revenue: number;
  cogs: number;
  grossMargin: number;
  operatingExpense: number;
  netIncome: number;
  cash: number;
  ar: number;
  ap: number;
  inventory: number;
  budget: number;
  budgetVariance: number;
  fxExposureThb: number;
  reconciledPercent: number;
  forecastCash: number;
}

export interface PipelineKpis {
  openSales: number;
  openProcurement: number;
  unpostedExpenses: number;
}

export interface ExecutiveFinance {
  filters: ReportFilter;
  actual: FinanceKpis;
  pipeline: PipelineKpis;
  arAging: Array<{ bucket: string; amount: number; count: number }>;
  apAging: Array<{ bucket: string; amount: number; count: number }>;
  revenueTrend: Array<{ period: string; revenue: number; grossMargin: number }>;
  stock: {
    quantity: number;
    value: number;
    expiringValue: number;
    turnover: number;
  };
  tieOuts: {
    debits: number;
    credits: number;
    journalsBalanced: boolean;
    assets: number;
    liabilitiesEquity: number;
    balanceSheetTied: boolean;
    arTied: boolean;
    apTied: boolean;
    inventoryTied: boolean;
  };
  generatedAt: string;
}

function validate(filter: ReportFilter) {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(filter.dateFrom) || !/^\d{4}-\d{2}-\d{2}$/.test(filter.dateTo)) {
    throw new Error('Report dates must be YYYY-MM-DD');
  }
  if (filter.dateFrom > filter.dateTo) throw new Error('Report start date cannot be after end date');
}

const n = (value: unknown) => Number(value ?? 0);

export async function loadExecutiveFinance(filter: ReportFilter): Promise<ExecutiveFinance> {
  validate(filter);
  const params = [filter.dateFrom, filter.dateTo, filter.branchId ?? null];
  const [actual, pipeline, ar, ap, trend, stock, tie, balance, forecast] = await Promise.all([
    query<{
      revenue: string;
      cogs: string;
      operating_expense: string;
      cash: string;
      ar: string;
      ap: string;
      inventory: string;
      fx_exposure: string;
      budget: string;
      reconciled_percent: string;
    }>(
      `WITH period_lines AS (
         SELECT * FROM finance.v_posted_lines
          WHERE posting_date BETWEEN $1::date AND $2::date
            AND ($3::bigint IS NULL OR branch_id = $3)
       ), balances AS (
         SELECT * FROM finance.v_posted_lines
          WHERE posting_date <= $2::date
            AND ($3::bigint IS NULL OR branch_id = $3)
       ), budget AS (
         SELECT coalesce(sum(bl.amount_thb), 0) AS total
           FROM finance.budget_lines bl
           JOIN finance.budgets b ON b.id = bl.budget_id AND b.status = 'approved'
          WHERE b.fiscal_year = extract(year from $2::date)
            AND bl.period_no BETWEEN extract(month from $1::date) AND extract(month from $2::date)
            AND ($3::bigint IS NULL OR b.branch_id IS NULL OR b.branch_id = $3)
       ), rec AS (
         SELECT count(*) FILTER (WHERE status = 'matched')::numeric AS matched,
                count(*)::numeric AS total
           FROM finance.bank_transactions t
          WHERE transaction_date BETWEEN $1::date AND $2::date
            AND ($3::bigint IS NULL OR EXISTS (
              SELECT 1 FROM finance.bank_accounts ba WHERE ba.id = t.bank_account_id AND ba.branch_id = $3
            ))
       )
       SELECT
         coalesce(sum(CASE WHEN account_type = 'revenue' THEN credit_thb - debit_thb ELSE 0 END), 0)::text AS revenue,
         coalesce(sum(CASE WHEN account_code LIKE '5101%' THEN debit_thb - credit_thb ELSE 0 END), 0)::text AS cogs,
         coalesce(sum(CASE WHEN account_type = 'expense' AND account_code NOT LIKE '5101%' THEN debit_thb - credit_thb ELSE 0 END), 0)::text AS operating_expense,
         (SELECT coalesce(sum(CASE WHEN control_type IN ('bank','cash') THEN debit_thb - credit_thb ELSE 0 END), 0) FROM balances)::text AS cash,
         (SELECT coalesce(sum(open_thb), 0) FROM finance.ar_balance_as_of($2::date, $3::bigint))::text AS ar,
         (SELECT coalesce(sum(open_thb), 0) FROM finance.ap_balance_as_of($2::date, $3::bigint))::text AS ap,
         (SELECT coalesce(sum(value_thb), 0) FROM inventory.valuation_as_of($2::date, $3::bigint))::text AS inventory,
         (SELECT coalesce(sum(abs(open_thb)), 0) FROM (
            SELECT open_thb FROM finance.ar_balance_as_of($2::date, $3::bigint) WHERE currency_code <> 'THB'
            UNION ALL
            SELECT open_thb FROM finance.ap_balance_as_of($2::date, $3::bigint) WHERE currency_code <> 'THB'
          ) exposure)::text AS fx_exposure,
         (SELECT total FROM budget)::text AS budget,
         (SELECT CASE WHEN total = 0 THEN 100 ELSE round(100 * matched / total, 1) END FROM rec)::text AS reconciled_percent
        FROM period_lines`,
      params,
    ),
    query<{ open_sales: string; open_procurement: string; unposted_expenses: string }>(
      `SELECT
         coalesce((SELECT sum(total_amount * CASE WHEN currency::text = 'THB' THEN 1 ELSE fx_rate END) FROM folio.sales_orders WHERE status NOT IN ('so_paid','rejected') AND ($1::bigint IS NULL OR branch_id = $1)), 0)::text AS open_sales,
         coalesce((SELECT sum(total_amount * CASE WHEN currency::text = 'THB' THEN 1 ELSE fx_rate END) FROM folio.purchase_orders WHERE status NOT IN ('disbursed','rejected') AND ($1::bigint IS NULL OR branch_id = $1)), 0)::text AS open_procurement,
         coalesce((SELECT sum(e.total_amount * CASE WHEN e.currency_code::text = 'THB' THEN 1 ELSE e.fx_rate END)
                     FROM folio.expenses e
                    WHERE e.status NOT IN ('completed','rejected')
                      AND ($1::bigint IS NULL OR e.branch_id = $1)
                      AND NOT EXISTS (
                        SELECT 1 FROM finance.journals j
                         WHERE j.source_type = 'expense' AND j.source_id = e.id::text AND j.status = 'posted'
                      )), 0)::text AS unposted_expenses`,
      [filter.branchId ?? null],
    ),
    query<{ aging_bucket: string; amount: string; count: string }>(
      `SELECT aging_bucket, amount::text, document_count::text AS count
         FROM finance.ar_aging($1::date, $2::bigint)
        ORDER BY CASE aging_bucket WHEN 'current' THEN 1 WHEN '1_30' THEN 2 WHEN '31_60' THEN 3 WHEN '61_90' THEN 4 ELSE 5 END`,
      [filter.dateTo, filter.branchId ?? null],
    ),
    query<{ aging_bucket: string; amount: string; count: string }>(
      `SELECT aging_bucket, amount::text, document_count::text AS count
         FROM finance.ap_aging($1::date, $2::bigint)
        ORDER BY CASE aging_bucket WHEN 'current' THEN 1 WHEN '1_30' THEN 2 WHEN '31_60' THEN 3 WHEN '61_90' THEN 4 ELSE 5 END`,
      [filter.dateTo, filter.branchId ?? null],
    ),
    query<{ period: string; revenue: string; gross_margin: string }>(
      `SELECT to_char(posting_date, 'YYYY-MM') AS period,
              sum(CASE WHEN account_type = 'revenue' THEN credit_thb - debit_thb ELSE 0 END)::text AS revenue,
              sum(CASE WHEN account_type = 'revenue' THEN credit_thb - debit_thb
                       WHEN account_code LIKE '5101%' THEN credit_thb - debit_thb ELSE 0 END)::text AS gross_margin
         FROM finance.v_posted_lines
        WHERE posting_date BETWEEN $1::date AND $2::date
          AND ($3::bigint IS NULL OR branch_id = $3)
        GROUP BY 1 ORDER BY 1`,
      params,
    ),
    query<{ quantity: string; value: string; expiring_value: string; cogs: string }>(
      `SELECT coalesce(sum(v.quantity), 0)::text AS quantity,
              coalesce(sum(v.value_thb), 0)::text AS value,
              coalesce(sum(v.value_thb) FILTER (WHERE l.expires_on BETWEEN $2::date AND $2::date + 90), 0)::text AS expiring_value,
              coalesce((SELECT sum(debit_thb - credit_thb) FROM finance.v_posted_lines
                         WHERE account_code LIKE '5101%' AND posting_date BETWEEN $1::date AND $2::date
                           AND ($3::bigint IS NULL OR branch_id = $3)), 0)::text AS cogs
         FROM inventory.valuation_as_of($2::date, $3::bigint) v
         LEFT JOIN inventory.lots l ON l.id = v.lot_id`,
      params,
    ),
    query<{
      total_debit: string;
      total_credit: string;
      journal_balanced: boolean;
      ar_tied: boolean;
      ap_tied: boolean;
      inventory_tied: boolean;
    }>(
      `WITH gl AS (
         SELECT coalesce(sum(debit_thb), 0) AS debit,
                coalesce(sum(credit_thb), 0) AS credit,
                coalesce(sum(CASE WHEN control_type = 'ar' THEN debit_thb - credit_thb ELSE 0 END), 0) AS ar,
                coalesce(sum(CASE WHEN control_type = 'ap' THEN credit_thb - debit_thb ELSE 0 END), 0) AS ap,
                coalesce(sum(CASE WHEN control_type = 'inventory' THEN debit_thb - credit_thb ELSE 0 END), 0) AS inventory
           FROM finance.v_posted_lines
          WHERE posting_date <= $1::date AND ($2::bigint IS NULL OR branch_id = $2)
       ), sub AS (
         SELECT coalesce((SELECT sum(open_thb) FROM finance.ar_balance_as_of($1::date, $2::bigint)), 0) AS ar,
                coalesce((SELECT sum(open_thb) FROM finance.ap_balance_as_of($1::date, $2::bigint)), 0) AS ap,
                coalesce((SELECT sum(value_thb) FROM inventory.valuation_as_of($1::date, $2::bigint)), 0) AS inventory
       )
       SELECT gl.debit::text AS total_debit,
              gl.credit::text AS total_credit,
              abs(gl.debit - gl.credit) < 0.01 AS journal_balanced,
              abs(gl.ar - sub.ar) < 0.01 AS ar_tied,
              abs(gl.ap - sub.ap) < 0.01 AS ap_tied,
              abs(gl.inventory - sub.inventory) < 0.01 AS inventory_tied
         FROM gl CROSS JOIN sub`,
      [filter.dateTo, filter.branchId ?? null],
    ),
    query<{ assets: string; liabilities_equity: string }>(
      `SELECT
         coalesce(sum(CASE WHEN account_type = 'asset' THEN debit_thb - credit_thb ELSE 0 END), 0)::text AS assets,
         coalesce(sum(CASE WHEN account_type IN ('liability','equity') THEN credit_thb - debit_thb
                           WHEN account_type = 'revenue' THEN credit_thb - debit_thb
                           WHEN account_type = 'expense' THEN credit_thb - debit_thb ELSE 0 END), 0)::text AS liabilities_equity
         FROM finance.v_posted_lines
        WHERE posting_date <= $1::date AND ($2::bigint IS NULL OR branch_id = $2)`,
      [filter.dateTo, filter.branchId ?? null],
    ),
    query<{ due_ar: string; due_ap: string; committed_po: string }>(
      `SELECT
         coalesce((SELECT sum(open_thb) FROM finance.ar_balance_as_of($1::date, $2::bigint) WHERE due_date <= $1::date + 30), 0)::text AS due_ar,
         coalesce((SELECT sum(open_thb) FROM finance.ap_balance_as_of($1::date, $2::bigint) WHERE due_date <= $1::date + 30), 0)::text AS due_ap,
         coalesce((SELECT sum(total_amount * CASE WHEN currency::text = 'THB' THEN 1 ELSE fx_rate END) FROM folio.purchase_orders WHERE status NOT IN ('disbursed','rejected') AND ($2::bigint IS NULL OR branch_id = $2)), 0)::text AS committed_po`,
      [filter.dateTo, filter.branchId ?? null],
    ),
  ]);

  const a = actual.rows[0];
  const p = pipeline.rows[0];
  const s = stock.rows[0];
  const t = tie.rows[0];
  const b = balance.rows[0];
  const f = forecast.rows[0];
  const revenue = n(a?.revenue);
  const cogs = n(a?.cogs);
  const operatingExpense = n(a?.operating_expense);
  const cash = n(a?.cash);
  const budget = n(a?.budget);
  const days = Math.max(1, (Date.parse(filter.dateTo) - Date.parse(filter.dateFrom)) / 86_400_000 + 1);
  const annualFactor = 365 / days;
  const avgStock = n(s?.value);
  return {
    filters: filter,
    actual: {
      revenue,
      cogs,
      grossMargin: revenue - cogs,
      operatingExpense,
      netIncome: revenue - cogs - operatingExpense,
      cash,
      ar: n(a?.ar),
      ap: n(a?.ap),
      inventory: n(a?.inventory),
      budget,
      budgetVariance: budget - operatingExpense,
      fxExposureThb: n(a?.fx_exposure),
      reconciledPercent: n(a?.reconciled_percent),
      forecastCash: cash + n(f?.due_ar) - n(f?.due_ap) - n(f?.committed_po),
    },
    pipeline: {
      openSales: n(p?.open_sales),
      openProcurement: n(p?.open_procurement),
      unpostedExpenses: n(p?.unposted_expenses),
    },
    arAging: ar.rows.map((row) => ({ bucket: row.aging_bucket, amount: n(row.amount), count: n(row.count) })),
    apAging: ap.rows.map((row) => ({ bucket: row.aging_bucket, amount: n(row.amount), count: n(row.count) })),
    revenueTrend: trend.rows.map((row) => ({ period: row.period, revenue: n(row.revenue), grossMargin: n(row.gross_margin) })),
    stock: {
      quantity: n(s?.quantity),
      value: n(s?.value),
      expiringValue: n(s?.expiring_value),
      turnover: avgStock === 0 ? 0 : n(s?.cogs) * annualFactor / avgStock,
    },
    tieOuts: {
      debits: n(t?.total_debit),
      credits: n(t?.total_credit),
      journalsBalanced: Boolean(t?.journal_balanced),
      assets: n(b?.assets),
      liabilitiesEquity: n(b?.liabilities_equity),
      balanceSheetTied: Math.abs(n(b?.assets) - n(b?.liabilities_equity)) < 0.01,
      arTied: Boolean(t?.ar_tied),
      apTied: Boolean(t?.ap_tied),
      inventoryTied: Boolean(t?.inventory_tied),
    },
    generatedAt: new Date().toISOString(),
  };
}

export async function loadBranches() {
  const result = await query<{ id: string; code: string; name: string }>(
    `SELECT id::text, code, name FROM finance.branches WHERE active ORDER BY code`,
  );
  return result.rows.map((row) => ({ id: Number(row.id), code: row.code, name: row.name }));
}
