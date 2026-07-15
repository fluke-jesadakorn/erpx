-- v5 schema: supervisor + account_supervisor roles + new approval flow
-- Adds roles for an extended 6-step approver chain:
--   Officer Staff -> Supervisor (if exists) -> Head of Department
--                -> Account Officer -> Account Supervisor (if exists)
--                -> Account Manager -> CFO
-- Additive only. Safe to re-run.

-- 1. New roles.
SELECT setval('roles_id_seq', GREATEST((SELECT MAX(id) FROM roles), 1));
INSERT INTO roles (name) VALUES ('supervisor')         ON CONFLICT (name) DO NOTHING;
INSERT INTO roles (name) VALUES ('account_supervisor') ON CONFLICT (name) DO NOTHING;
SELECT setval('roles_id_seq', GREATEST((SELECT MAX(id) FROM roles), 1));

-- 2. New statuses for expenses + prs.
--    These are just VARCHAR columns with no CHECK, so the values are
--    'valid by convention' — the engine validates them.

-- 3. Seed sample supervisors so the chain has actors out of the box.
INSERT INTO users (employee_code, fullname, role_id, department, department_id, is_active)
SELECT 'EMP017', 'Steven Supervisor', r.id, 'Engineering',
       (SELECT id FROM departments WHERE code='ENG'), TRUE
FROM roles r WHERE r.name='supervisor'
ON CONFLICT (employee_code) DO UPDATE SET
  fullname = EXCLUDED.fullname,
  role_id  = EXCLUDED.role_id,
  is_active = TRUE;

INSERT INTO users (employee_code, fullname, role_id, department, department_id, is_active)
SELECT 'EMP018', 'Andrew Supervisor', r.id, 'Finance & Account',
       (SELECT id FROM departments WHERE code='FIN'), TRUE
FROM roles r WHERE r.name='account_supervisor'
ON CONFLICT (employee_code) DO UPDATE SET
  fullname = EXCLUDED.fullname,
  role_id  = EXCLUDED.role_id,
  is_active = TRUE;

-- 4. Wire the new supervisors into the org chart.
--    Supervisor EMP017 reports to head_of_department EMP002 (Engineering head).
UPDATE users
SET reports_to_user_id = (SELECT id FROM users WHERE employee_code='EMP002')
WHERE employee_code='EMP017' AND reports_to_user_id IS NULL;

-- Have Engineering staff (EMP001) report to supervisor EMP017 instead of HoD EMP002.
UPDATE users
SET reports_to_user_id = (SELECT id FROM users WHERE employee_code='EMP017')
WHERE employee_code='EMP001';

-- Account Supervisor EMP018 reports to Accounting Manager EMP004.
UPDATE users
SET reports_to_user_id = (SELECT id FROM users WHERE employee_code='EMP004')
WHERE employee_code='EMP018' AND reports_to_user_id IS NULL;

-- Account Officer EMP003 reports to Account Supervisor EMP018.
UPDATE users
SET reports_to_user_id = (SELECT id FROM users WHERE employee_code='EMP018')
WHERE employee_code='EMP003' AND reports_to_user_id IS NULL;

-- 5. Default policy with the 6-step chain. Engine skips stages whose role
--    has zero users in the org ("if exists" semantics).
INSERT INTO approval_policies (name, priority, target_type, conditions_json, action_json, created_by)
SELECT
    'Standard chain: Supervisor -> HoD -> AO -> AS -> AM -> CFO',
    35,
    'both',
    '{"all_of":[]}'::jsonb,
    '{
       "approver_chain": [
         "supervisor",
         "head_of_department",
         "account_officer",
         "account_supervisor",
         "accounting_manager",
         "cfo"
       ],
       "auto_approve": false,
       "notify": ["requester"]
     }'::jsonb,
    (SELECT id FROM users WHERE employee_code='EMP005')
WHERE NOT EXISTS (
  SELECT 1 FROM approval_policies
  WHERE name = 'Standard chain: Supervisor -> HoD -> AO -> AS -> AM -> CFO'
);
