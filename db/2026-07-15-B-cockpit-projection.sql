-- 2026-07-15 B: adds public.get_cockpit_projection(days_ahead) for linear-regression cash/MTD forecast from exec_snapshots, plus public.backfill_exec_snapshot(date, cash, mtd) for synthetic seeding.

BEGIN;

CREATE OR REPLACE FUNCTION public.backfill_exec_snapshot(
  target_date date,
  cash_val    numeric,
  mtd_val     numeric
) RETURNS void AS $$
  INSERT INTO exec_snapshots (
    snapshot_date,
    kpis,
    dept_budgets,
    stuck_count
  ) VALUES (
    target_date,
    jsonb_build_object(
      'cash', jsonb_build_object(
        'totalCash',               cash_val,
        'outstandingLiabilities',  0,
        'mtdExpenses',             mtd_val,
        'netIncome',               0
      ),
      'kpis',       jsonb_build_object('mtdExpenses', mtd_val),
      'cashTrend',  jsonb_build_array(cash_val),
      'mtdTrend',   jsonb_build_array(mtd_val)
    ),
    '[]'::jsonb,
    0
  )
  ON CONFLICT (snapshot_date) DO NOTHING;
$$ LANGUAGE sql VOLATILE;

CREATE OR REPLACE FUNCTION public.get_cockpit_projection(
  days_ahead int DEFAULT 90
) RETURNS jsonb AS $$
WITH series AS (
  SELECT
    snapshot_date                                         AS d,
    (kpis->'cash'->>'totalCash')::numeric                 AS cash,
    (kpis->'kpis'->>'mtdExpenses')::numeric               AS mtd,
    EXTRACT(EPOCH FROM snapshot_date)::numeric / 86400    AS x
  FROM exec_snapshots
),
regs AS (
  SELECT
    COALESCE(regr_slope    (cash, x), 0)::numeric AS cash_slope,
    COALESCE(regr_intercept(cash, x), 0)::numeric AS cash_intercept,
    COALESCE(regr_r2       (cash, x), 0)::numeric AS cash_r2,
    COALESCE(regr_slope    (mtd,  x), 0)::numeric AS mtd_slope,
    COALESCE(regr_intercept(mtd,  x), 0)::numeric AS mtd_intercept,
    COALESCE(regr_r2       (mtd,  x), 0)::numeric AS mtd_r2
  FROM series
),
proj AS (
  SELECT
    gs::int                                                       AS offset_days,
    (CURRENT_DATE + gs)::date                                     AS pd,
    EXTRACT(EPOCH FROM (CURRENT_DATE + gs)::date)::numeric / 86400 AS px,
    r.cash_slope, r.cash_intercept, r.mtd_slope, r.mtd_intercept
  FROM generate_series(1, days_ahead) gs
  CROSS JOIN regs r
)
SELECT jsonb_build_object(
  'historical', COALESCE(
    (SELECT jsonb_agg(
       jsonb_build_object('date', d::text, 'cash', cash, 'mtd', mtd)
       ORDER BY d)
     FROM series),
    '[]'::jsonb),
  'regression', jsonb_build_object(
    'cash', jsonb_build_object(
      'slope',     cash_slope,
      'intercept', cash_intercept,
      'r2',        cash_r2),
    'mtd',  jsonb_build_object(
      'slope',     mtd_slope,
      'intercept', mtd_intercept,
      'r2',        mtd_r2)
  ),
  'projection', COALESCE(
    (SELECT jsonb_agg(
       jsonb_build_object(
         'date',           pd::text,
         'cashProjected',  ROUND(cash_slope * px + cash_intercept, 2),
         'mtdProjected',   ROUND(mtd_slope  * px + mtd_intercept,  2),
         'isProjected',    true)
       ORDER BY offset_days)
     FROM proj),
    '[]'::jsonb),
  'summary', jsonb_build_object(
    'currentCash',     COALESCE((SELECT cash FROM series ORDER BY d DESC LIMIT 1), 0),
    'currentMtd',      COALESCE((SELECT mtd  FROM series ORDER BY d DESC LIMIT 1), 0),
    'projectedCash30', ROUND(cash_slope * (EXTRACT(EPOCH FROM (CURRENT_DATE + 30)::date)::numeric / 86400) + cash_intercept, 2),
    'projectedCash60', ROUND(cash_slope * (EXTRACT(EPOCH FROM (CURRENT_DATE + 60)::date)::numeric / 86400) + cash_intercept, 2),
    'projectedCash90', ROUND(cash_slope * (EXTRACT(EPOCH FROM (CURRENT_DATE + 90)::date)::numeric / 86400) + cash_intercept, 2),
    'monthlyBurn',     ROUND(cash_slope * 30, 2),
    'daysToZero',      CASE
                         WHEN cash_slope < 0
                           THEN ROUND(
                             ABS(
                               (0 - COALESCE((SELECT cash FROM series ORDER BY d DESC LIMIT 1), 0))::numeric
                               / cash_slope
                             )
                           )::int
                         ELSE NULL
                       END,
    'trend', CASE
               WHEN cash_slope < -1 THEN 'down'
               WHEN cash_slope >  1 THEN 'up'
               ELSE 'flat'
             END,
    'r2', cash_r2
  )
) FROM regs;
$$ LANGUAGE sql STABLE;

COMMIT;
