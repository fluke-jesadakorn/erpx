import 'server-only';
import { cache } from 'react';
import { query } from '../db';

export interface ProjectionPoint {
  date: string;
  cash?: number;
  mtd?: number;
  revenue?: number;
  cashProjected?: number;
  mtdProjected?: number;
  revenueProjected?: number;
  isProjected?: boolean;
}

export interface ProjectionRegression {
  cash: { slope: number; intercept: number; r2: number };
  mtd:  { slope: number; intercept: number; r2: number };
  revenue: { slope: number; intercept: number; r2: number };
}

export interface ProjectionSummary {
  currentCash: number;
  currentMtd: number;
  revenue30: number;
  revenue60: number;
  revenue90: number;
  projectedCash30: number;
  projectedCash60: number;
  projectedCash90: number;
  monthlyBurn: number;
  daysToZero: number | null;
  trend: 'up' | 'down' | 'flat';
  r2: number;
}

export interface ProjectionResult {
  historical: ProjectionPoint[];
  regression: ProjectionRegression;
  projection: ProjectionPoint[];
  summary: ProjectionSummary;
}

const FALLBACK: ProjectionResult = {
  historical: [],
  regression: {
    cash: { slope: 0, intercept: 0, r2: 0 },
    mtd: { slope: 0, intercept: 0, r2: 0 },
    revenue: { slope: 0, intercept: 0, r2: 0 },
  },
  projection: [],
  summary: {
    currentCash: 0, currentMtd: 0,
    revenue30: 0, revenue60: 0, revenue90: 0,
    projectedCash30: 0, projectedCash60: 0, projectedCash90: 0,
    monthlyBurn: 0, daysToZero: null, trend: 'flat', r2: 0,
  },
};

async function loadRevenueWindows(): Promise<{ r30: number; r60: number; r90: number }> {
  try {
    const r = await query<{ r30: string; r60: string; r90: string }>(
      `SELECT
         COALESCE(SUM(CASE WHEN j.entry_date >= CURRENT_DATE - INTERVAL '30 days' THEN l.credit ELSE 0 END), 0)::numeric::float8 AS r30,
         COALESCE(SUM(CASE WHEN j.entry_date >= CURRENT_DATE - INTERVAL '60 days' THEN l.credit ELSE 0 END), 0)::numeric::float8 AS r60,
         COALESCE(SUM(CASE WHEN j.entry_date >= CURRENT_DATE - INTERVAL '90 days' THEN l.credit ELSE 0 END), 0)::numeric::float8 AS r90
       FROM ledger_lines l
       JOIN journal_entries j ON j.id = l.journal_entry_id
       JOIN chart_of_accounts c ON c.code = l.account_code
      WHERE c.account_type = 'revenue'`,
    );
    const row = r.rows[0];
    return {
      r30: Number(row?.r30 ?? 0),
      r60: Number(row?.r60 ?? 0),
      r90: Number(row?.r90 ?? 0),
    };
  } catch (error: any) {
    console.error('loadRevenueWindows failed:', error);
    return { r30: 0, r60: 0, r90: 0 };
  }
}

async function loadProjectionInternal(daysAhead: number): Promise<ProjectionResult> {
  try {
    const r = await query<{ get_cockpit_projection: ProjectionResult | null }>(
      `SELECT get_cockpit_projection($1::int) AS get_cockpit_projection`,
      [daysAhead],
    );
    const row = r.rows[0]?.get_cockpit_projection;
    const base: ProjectionResult = row ? (row as ProjectionResult) : FALLBACK;
    const rev = await loadRevenueWindows();
    const summary: ProjectionSummary = {
      ...base.summary,
      revenue30: rev.r30,
      revenue60: rev.r60,
      revenue90: rev.r90,
    };
    return { ...base, summary };
  } catch {
    const rev = await loadRevenueWindows();
    return {
      ...FALLBACK,
      summary: { ...FALLBACK.summary, revenue30: rev.r30, revenue60: rev.r60, revenue90: rev.r90 },
    };
  }
}

export const loadProjection = cache((daysAhead = 90) => loadProjectionInternal(daysAhead));
