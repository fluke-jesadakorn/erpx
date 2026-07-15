BEGIN;

INSERT INTO perm.roles (id, display_name, monthly_budget, is_system)
VALUES
  ('dept-development', 'Development', 2000000, false),
  ('dept-marketing',   'Marketing',   2000000, false),
  ('dept-finance',     'Finance',     2000000, false),
  ('dept-executive',   'Executive',   1000000, false),
  ('dept-hr',          'HR',           500000, false),
  ('dept-it',          'IT',           300000, false)
ON CONFLICT (id) DO UPDATE SET monthly_budget = EXCLUDED.monthly_budget;

COMMIT;