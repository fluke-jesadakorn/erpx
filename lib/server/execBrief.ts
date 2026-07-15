import 'server-only';
import { cache } from 'react';
import { query } from '../db';

export interface ExecBriefKpis {
  totalCash: number;
  outstandingLiabilities: number;
  mtdExpenses: number;
  netIncome: number;
  mtdTrend: number[];
  cashTrend: number[];
  salesRevenueMtd: number;
  salesRevenueTrend: number[];
  openSalesOrders: number;
  overdueArAmount: number;
}

export interface ArAgingBucket {
  bucket: string;
  amount_thb: number;
  so_count: number;
}

export interface ExecBriefDeptBudget {
  dept_id: string;
  dept_name: string;
  monthly_budget: number;
  mtd_spend: number;
  pct_used: number;
  is_over_threshold: boolean;
}

export interface ExecutiveBrief {
  kpis: ExecBriefKpis;
  deptBudgets: ExecBriefDeptBudget[];
  stuckCount: number;
  arAging: ArAgingBucket[];
  generatedAt: string;
}

async function loadKpis(): Promise<Omit<ExecBriefKpis, 'mtdTrend' | 'cashTrend'>> {
  const r = await query<{
    total_cash: string;
    outstanding_liabilities: string;
    mtd_expenses: string;
    net_income: string;
  }>(
    `WITH coa_balance AS (
       SELECT c.code, c.account_type,
              COALESCE(SUM(l.debit), 0)::numeric  AS total_debit,
              COALESCE(SUM(l.credit), 0)::numeric AS total_credit
         FROM chart_of_accounts c
         LEFT JOIN ledger_lines l ON c.code = l.account_code
         GROUP BY c.code, c.account_type
     ),
     mtd_expenses AS (
       SELECT COALESCE(SUM(l.debit), 0)::numeric AS mtd
         FROM ledger_lines l
         JOIN journal_entries j ON l.journal_entry_id = j.id
         JOIN chart_of_accounts c ON c.code = l.account_code
        WHERE c.account_type = 'expense'
          AND j.entry_date >= DATE_TRUNC('month', CURRENT_DATE)
     )
     SELECT
       COALESCE(SUM(CASE WHEN account_type IN ('asset','expense') THEN total_debit - total_credit ELSE 0 END), 0)::numeric -
       COALESCE(SUM(CASE WHEN account_type IN ('liability','equity','revenue') THEN total_credit - total_debit ELSE 0 END), 0)::numeric
       AS net_income,
       COALESCE(SUM(CASE WHEN code IN ('110100','110200','110300') AND account_type = 'asset' THEN total_debit - total_credit ELSE 0 END), 0)::numeric
       AS total_cash,
       COALESCE(SUM(CASE WHEN account_type = 'liability' THEN total_credit - total_debit ELSE 0 END), 0)::numeric
       AS outstanding_liabilities,
       (SELECT mtd FROM mtd_expenses) AS mtd_expenses
       FROM coa_balance`,
  );
  const row = r.rows[0] || {} as any;
  return {
    totalCash: Number(row.total_cash ?? 0),
    outstandingLiabilities: Number(row.outstanding_liabilities ?? 0),
    mtdExpenses: Number(row.mtd_expenses ?? 0),
    netIncome: Number(row.net_income ?? 0),
    salesRevenueMtd: 0,
    salesRevenueTrend: [],
    openSalesOrders: 0,
    overdueArAmount: 0,
  };
}

async function loadTrend(): Promise<{ mtdTrend: number[]; cashTrend: number[] }> {
  const r = await query<{ cash: string | null; mtd: string | null }>(
    `SELECT kpis->'cash'->>'totalCash' AS cash,
            kpis->'kpis'->>'mtdExpenses' AS mtd
       FROM exec_snapshots
      WHERE snapshot_date >= CURRENT_DATE - INTERVAL '7 days'
      ORDER BY snapshot_date ASC`,
  );
  const cashTrend: number[] = [];
  const mtdTrend: number[] = [];
  for (const row of r.rows) {
    cashTrend.push(Number(row.cash ?? 0));
    mtdTrend.push(Number(row.mtd ?? 0));
  }
  return { cashTrend, mtdTrend };
}

async function loadDeptBudgets(year: number, month: number): Promise<ExecBriefDeptBudget[]> {
  const r = await query<{
    dept_id: string;
    dept_name: string;
    monthly_budget: string;
    mtd_spend: string;
    pct_used: string;
    is_over_threshold: boolean;
  }>(
    `SELECT * FROM get_dept_budget_status($1::int, $2::int)`,
    [year, month],
  );
  return r.rows.map((row) => ({
    dept_id: row.dept_id,
    dept_name: row.dept_name,
    monthly_budget: Number(row.monthly_budget ?? 0),
    mtd_spend: Number(row.mtd_spend ?? 0),
    pct_used: Number(row.pct_used ?? 0),
    is_over_threshold: !!row.is_over_threshold,
  }));
}

async function loadStuckCount(): Promise<number> {
  const r = await query<{ n: number }>(
    `SELECT COUNT(*)::int AS n
       FROM expenses e
       JOIN waybills w
         ON w.origin = 'expense' AND w.origin_id = e.id
      WHERE w.current_stage NOT IN ('disbursed','rejected','gl_confirmed')
        AND e.created_at < NOW() - INTERVAL '24 hours'`,
  );
  return r.rows[0]?.n ?? 0;
}

async function loadSalesKpis(): Promise<{
  salesRevenueMtd: number;
  salesRevenueTrend: number[];
  openSalesOrders: number;
  overdueArAmount: number;
}> {
  const mtdRes = await query<{ mtd: string | null }>(
    `SELECT COALESCE(SUM(total_amount), 0)::numeric::float8 AS mtd
       FROM sales_orders
      WHERE status = 'so_paid'
        AND paid_at >= DATE_TRUNC('month', CURRENT_DATE)`,
  );
  const trendRes = await query<{ day: string; amount: string | null }>(
    `WITH days AS (
       SELECT generate_series(CURRENT_DATE - INTERVAL '6 days', CURRENT_DATE, INTERVAL '1 day')::date AS day
     )
     SELECT d.day::text AS day,
            COALESCE(SUM(so.total_amount), 0)::numeric::float8 AS amount
       FROM days d
  LEFT JOIN sales_orders so
         ON so.status = 'so_paid'
        AND so.paid_at::date = d.day
   GROUP BY d.day
   ORDER BY d.day ASC`,
  );
  const openRes = await query<{ n: number }>(
    `SELECT COUNT(*)::int AS n
       FROM sales_orders
      WHERE status IN ('so_draft','so_sales_review','so_credit_check','so_invoiced')`,
  );
  const overdueRes = await query<{ v: string | null }>(
    `SELECT COALESCE(SUM(total_amount), 0)::numeric::float8 AS v
       FROM sales_orders
      WHERE status = 'so_invoiced'
        AND due_date IS NOT NULL
        AND due_date < CURRENT_DATE`,
  );
  return {
    salesRevenueMtd: Number(mtdRes.rows[0]?.mtd ?? 0),
    salesRevenueTrend: trendRes.rows.map((r) => Number(r.amount ?? 0)),
    openSalesOrders: Number(openRes.rows[0]?.n ?? 0),
    overdueArAmount: Number(overdueRes.rows[0]?.v ?? 0),
  };
}

async function loadArAging(): Promise<ArAgingBucket[]> {
  try {
    const r = await query<{ bucket: string; amount_thb: string; so_count: string }>(
      `SELECT bucket, amount_thb, so_count FROM get_ar_aging_buckets()`,
    );
    return r.rows.map((row) => ({
      bucket: row.bucket,
      amount_thb: Number(row.amount_thb ?? 0),
      so_count: Number(row.so_count ?? 0),
    }));
  } catch (error: any) {
    console.error('loadArAging failed:', error);
    return [];
  }
}

async function loadBriefInternal(actor: { id?: number }): Promise<ExecutiveBrief> {
  void actor;
  const now = new Date();
  const year = now.getFullYear();
  const month = now.getMonth() + 1;

  const [kpisRaw, trend, deptBudgets, stuckCount, salesKpis, arAging] = await Promise.all([
    loadKpis(),
    loadTrend(),
    loadDeptBudgets(year, month),
    loadStuckCount(),
    loadSalesKpis(),
    loadArAging(),
  ]);

  return {
    kpis: {
      ...kpisRaw,
      mtdTrend: trend.mtdTrend,
      cashTrend: trend.cashTrend,
      salesRevenueMtd: salesKpis.salesRevenueMtd,
      salesRevenueTrend: salesKpis.salesRevenueTrend,
      openSalesOrders: salesKpis.openSalesOrders,
      overdueArAmount: salesKpis.overdueArAmount,
    },
    deptBudgets,
    stuckCount,
    arAging,
    generatedAt: now.toISOString(),
  };
}

export const loadExecutiveBrief = cache(loadBriefInternal);
