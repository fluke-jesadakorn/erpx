-- v2 seed: new roles, users, departments, default policies

-- New roles (keep existing 1-4 intact)
INSERT INTO roles (id, name) VALUES
    (5, 'head_of_department'),
    (6, 'accounting_manager'),
    (7, 'cfo'),
    (8, 'ceo'),
    (9, 'account_officer')
ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name;
INSERT INTO roles (id, name) VALUES
    (2, 'accountant')
ON CONFLICT (id) DO NOTHING;

-- New users
INSERT INTO users (employee_code, fullname, role_id, department) VALUES
    ('EMP002', 'Sarah Approver',     5, 'Engineering'),
    ('EMP003', 'Mark Reviewer',    9, 'Finance & Account'),
    ('EMP004', 'Emily Manager', 6, 'Finance & Account'),
    ('EMP005', 'Olivia Director',  7, 'Executive'),
    ('EMP006', 'Charles Executive',  8, 'Executive'),
    ('EMP007', 'Lisa Staff',     1, 'Sales'),
    ('EMP008', 'David Approver',    5, 'Sales'),
    ('EMP009', 'Robert Reviewer',        9, 'Finance & Account'),
    ('EMP010', 'Karen Staff',        1, 'Marketing')
ON CONFLICT (employee_code) DO UPDATE SET
    fullname = EXCLUDED.fullname,
    role_id = EXCLUDED.role_id,
    department = EXCLUDED.department;

-- Departments (idempotent)
INSERT INTO departments (code, name, monthly_budget) VALUES
    ('DEV', 'Development',     150000.00),
    ('ENG', 'Engineering',     120000.00),
    ('SAL', 'Sales',           100000.00),
    ('MKT', 'Marketing',        80000.00),
    ('HR',  'Human Resource',   50000.00),
    ('FIN', 'Finance & Account',70000.00),
    ('EXC', 'Executive',       200000.00)
ON CONFLICT (code) DO UPDATE SET
    name = EXCLUDED.name,
    monthly_budget = EXCLUDED.monthly_budget;

-- Wire HoD into departments
UPDATE departments SET head_user_id = (SELECT id FROM users WHERE employee_code = 'EMP002') WHERE code = 'ENG';
UPDATE departments SET head_user_id = (SELECT id FROM users WHERE employee_code = 'EMP008') WHERE code = 'SAL';
UPDATE departments SET head_user_id = (SELECT id FROM users WHERE employee_code = 'EMP002') WHERE code = 'DEV';

-- Default approval policies (priority 10..50; lower = first)
-- Policy 10: <= 5,000 THB auto-approve
INSERT INTO approval_policies (name, priority, target_type, conditions_json, action_json, created_by)
SELECT
    'Auto-Approve small expenses (≤5,000 THB)',
    10,
    'expense',
    '{"all_of":[{"field":"total_amount","op":"lte","value":5000}]}'::jsonb,
    '{"approver_chain":[],"auto_approve":true,"notify":["requester","cfo"]}'::jsonb,
    (SELECT id FROM users WHERE employee_code = 'EMP005')
WHERE NOT EXISTS (SELECT 1 FROM approval_policies WHERE name = 'Auto-Approve small expenses (≤5,000 THB)');

-- Policy 20: 5,001-50,000 THB -> HoD
INSERT INTO approval_policies (name, priority, target_type, conditions_json, action_json, created_by)
SELECT
    'Department approval (5,001-50,000 THB)',
    20,
    'both',
    '{"all_of":[{"field":"total_amount","op":"between","value":[5001,50000]}]}'::jsonb,
    '{"approver_chain":["head_of_department"],"auto_approve":false,"notify":["requester"]}'::jsonb,
    (SELECT id FROM users WHERE employee_code = 'EMP005')
WHERE NOT EXISTS (SELECT 1 FROM approval_policies WHERE name = 'Department approval (5,001-50,000 THB)');

-- Policy 30: 50,001-200,000 THB -> HoD + Accounting Manager
INSERT INTO approval_policies (name, priority, target_type, conditions_json, action_json, created_by)
SELECT
    'Mid-tier approval (50,001-200,000 THB)',
    30,
    'both',
    '{"all_of":[{"field":"total_amount","op":"between","value":[50001,200000]}]}'::jsonb,
    '{"approver_chain":["head_of_department","accounting_manager"],"auto_approve":false,"notify":["requester","cfo"]}'::jsonb,
    (SELECT id FROM users WHERE employee_code = 'EMP005')
WHERE NOT EXISTS (SELECT 1 FROM approval_policies WHERE name = 'Mid-tier approval (50,001-200,000 THB)');

-- Policy 40: > 200,000 OR high-impact categories -> full chain
INSERT INTO approval_policies (name, priority, target_type, conditions_json, action_json, created_by)
SELECT
    'Executive approval (>200,000 THB or sensitive category)',
    40,
    'both',
    '{"any_of":[{"field":"total_amount","op":"gt","value":200000},{"field":"category_code","op":"in","value":["520100","520200","520300"]}]}'::jsonb,
    '{"approver_chain":["head_of_department","accounting_manager","cfo"],"auto_approve":false,"notify":["requester","ceo"]}'::jsonb,
    (SELECT id FROM users WHERE employee_code = 'EMP005')
WHERE NOT EXISTS (SELECT 1 FROM approval_policies WHERE name = 'Executive approval (>200,000 THB or sensitive category)');

-- Policy 50: catch-all default
INSERT INTO approval_policies (name, priority, target_type, conditions_json, action_json, created_by)
SELECT
    'Default departmental chain',
    50,
    'both',
    '{"all_of":[]}'::jsonb,
    '{"approver_chain":["head_of_department","accounting_manager"],"auto_approve":false,"notify":["requester"]}'::jsonb,
    (SELECT id FROM users WHERE employee_code = 'EMP005')
WHERE NOT EXISTS (SELECT 1 FROM approval_policies WHERE name = 'Default departmental chain');

-- No-op: department names are already aligned; IDs are resolved at query time.
