BEGIN;

DROP FUNCTION IF EXISTS public.get_dept_budget_status(int, int);

CREATE OR REPLACE FUNCTION public.get_dept_budget_status(
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
  WITH dept_users AS (
    SELECT DISTINCT regexp_replace(permission_id, '^user:dept:([^:]+)::allow$', '\1') AS dept_id,
           up.user_id
      FROM perm.user_permissions up
     WHERE permission_id LIKE 'user:dept:%::allow'
       AND revoked_at IS NULL
  ),
  depts AS (
    SELECT d.dept_id,
           COALESCE(MAX(r.monthly_budget), 0)::numeric AS monthly_budget,
           COALESCE(MAX(r.display_name), initcap(replace(d.dept_id, '-', ' '))) AS display_name
      FROM (SELECT DISTINCT dept_id FROM dept_users) d
      LEFT JOIN perm.roles r ON r.id = 'dept-' || d.dept_id
     GROUP BY d.dept_id
  )
  SELECT d.dept_id,
         d.display_name,
         d.monthly_budget,
         COALESCE(SUM(e.total_amount) FILTER (
           WHERE EXTRACT(YEAR  FROM e.created_at) = p_fiscal_year
             AND EXTRACT(MONTH FROM e.created_at) = p_month
             AND e.status NOT IN ('rejected','draft')
         ), 0)::numeric AS mtd_spend,
         CASE
           WHEN d.monthly_budget > 0
             THEN ROUND((COALESCE(SUM(e.total_amount) FILTER (
               WHERE EXTRACT(YEAR  FROM e.created_at) = p_fiscal_year
                 AND EXTRACT(MONTH FROM e.created_at) = p_month
                 AND e.status NOT IN ('rejected','draft')
             ), 0) / d.monthly_budget * 100)::numeric, 1)
           ELSE 0
         END AS pct_used,
         CASE
           WHEN d.monthly_budget > 0
             THEN COALESCE(SUM(e.total_amount) FILTER (
               WHERE EXTRACT(YEAR  FROM e.created_at) = p_fiscal_year
                 AND EXTRACT(MONTH FROM e.created_at) = p_month
                 AND e.status NOT IN ('rejected','draft')
             ), 0) > (d.monthly_budget * 0.9)
           ELSE false
         END AS is_over_threshold
    FROM depts d
    LEFT JOIN dept_users du ON du.dept_id = d.dept_id
    LEFT JOIN expenses e ON e.submitter_id = du.user_id
   GROUP BY d.dept_id, d.display_name, d.monthly_budget
   ORDER BY d.display_name;
$$ LANGUAGE sql STABLE;

COMMIT;