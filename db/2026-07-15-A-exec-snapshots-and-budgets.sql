BEGIN;

ALTER TABLE perm.roles
  ADD COLUMN IF NOT EXISTS monthly_budget NUMERIC(14, 2) NOT NULL DEFAULT 0;

UPDATE perm.roles SET monthly_budget = 2000000 WHERE id = 'dept-development' AND monthly_budget = 0;
UPDATE perm.roles SET monthly_budget = 2000000 WHERE id = 'dept-marketing'   AND monthly_budget = 0;
UPDATE perm.roles SET monthly_budget = 2000000 WHERE id = 'dept-finance-2'   AND monthly_budget = 0;
UPDATE perm.roles SET monthly_budget = 1000000 WHERE id = 'dept-executive'   AND monthly_budget = 0;
UPDATE perm.roles SET monthly_budget =  500000 WHERE id = 'dept-hr-2'        AND monthly_budget = 0;
UPDATE perm.roles SET monthly_budget =  300000 WHERE id = 'dept-it'          AND monthly_budget = 0;

DROP FUNCTION IF EXISTS get_dept_budget_status(int, int);

CREATE OR REPLACE FUNCTION get_dept_budget_status(
  p_fiscal_year int,
  p_month       int
) RETURNS TABLE (
  dept_id          text,
  dept_name        text,
  monthly_budget   numeric,
  mtd_spend        numeric,
  pct_used         numeric,
  is_over_threshold boolean
) AS $$
  SELECT
    g.id,
    g.display_name,
    g.monthly_budget,
    COALESCE(SUM(e.total_amount) FILTER (
      WHERE EXTRACT(YEAR  FROM e.created_at) = p_fiscal_year
        AND EXTRACT(MONTH FROM e.created_at) = p_month
        AND e.status NOT IN ('rejected','draft')
    ), 0)::numeric AS mtd_spend,
    CASE
      WHEN g.monthly_budget > 0
        THEN ROUND((COALESCE(SUM(e.total_amount) FILTER (
          WHERE EXTRACT(YEAR  FROM e.created_at) = p_fiscal_year
            AND EXTRACT(MONTH FROM e.created_at) = p_month
            AND e.status NOT IN ('rejected','draft')
        ), 0) / g.monthly_budget * 100)::numeric, 1)
      ELSE 0
    END AS pct_used,
    COALESCE(SUM(e.total_amount) FILTER (
      WHERE EXTRACT(YEAR  FROM e.created_at) = p_fiscal_year
        AND EXTRACT(MONTH FROM e.created_at) = p_month
        AND e.status NOT IN ('rejected','draft')
    ), 0) > (g.monthly_budget * 0.9)
  FROM perm.roles g
  LEFT JOIN users u ON u.dept_group_id = g.id
  LEFT JOIN expenses e ON e.submitter_id = u.id
  WHERE g.kind = 'department'
  GROUP BY g.id, g.display_name, g.monthly_budget
  ORDER BY g.display_name;
$$ LANGUAGE sql STABLE;

CREATE TABLE IF NOT EXISTS exec_snapshots (
  id             bigserial PRIMARY KEY,
  snapshot_date  date NOT NULL UNIQUE,
  kpis           jsonb NOT NULL,
  dept_budgets   jsonb NOT NULL DEFAULT '[]'::jsonb,
  stuck_count    int  NOT NULL DEFAULT 0,
  created_at     timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS exec_snapshots_date_idx
  ON exec_snapshots (snapshot_date DESC);

COMMIT;
