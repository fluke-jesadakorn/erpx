BEGIN;

SET search_path TO folio, public;

CREATE TEMP TABLE access_bootstrap AS
SELECT u.id AS user_id,
       replace(replace(up.permission_id, 'user:dept:', ''), '::allow', '') AS department_id,
       CASE
         WHEN bool_or(ur.role_id = 'it::2') THEN 'director'
         WHEN bool_or(ur.role_id IN ('hr_manager::3', 'manager::3')) THEN 'manager'
         WHEN bool_or(ur.role_id = 'supervisor::4') THEN 'supervisor'
         ELSE 'officer'
       END AS hierarchy_role_id,
       bool_or(ur.role_id = 'it::2') AS system_admin
  FROM users u
  JOIN perm.user_permissions up ON up.user_id = u.id
   AND up.permission_id IN ('user:dept:hr::allow', 'user:dept:it::allow')
   AND up.revoked_at IS NULL
   AND up.starts_at <= now()
   AND (up.ends_at IS NULL OR up.ends_at > now())
  LEFT JOIN perm.user_roles ur ON ur.user_id = u.id
 WHERE u.is_active IS TRUE
 GROUP BY u.id, up.permission_id;

DROP TABLE IF EXISTS perm.department_permissions;
DROP TABLE IF EXISTS perm.user_departments;
DROP TABLE IF EXISTS perm.user_roles;
DROP TABLE IF EXISTS perm.role_permissions;
DROP TABLE IF EXISTS perm.roles;

CREATE TABLE perm.departments (
  id text PRIMARY KEY,
  display_name text NOT NULL UNIQUE,
  head_user_id integer REFERENCES users(id) ON DELETE SET NULL,
  is_system boolean NOT NULL DEFAULT true,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  CHECK (id ~ '^[a-z][a-z0-9_-]*$')
);

CREATE TABLE perm.roles (
  id text PRIMARY KEY,
  display_name text NOT NULL UNIQUE,
  description text,
  kind text NOT NULL,
  rank smallint,
  is_system boolean NOT NULL DEFAULT true,
  sort_order integer NOT NULL DEFAULT 0,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (id, kind),
  CHECK (id ~ '^[a-z][a-z0-9_-]*$'),
  CHECK (kind IN ('hierarchy', 'system')),
  CHECK ((kind = 'hierarchy' AND rank BETWEEN 1 AND 7) OR (kind = 'system' AND rank IS NULL))
);

CREATE TABLE perm.role_permissions (
  role_id text NOT NULL,
  role_kind text NOT NULL,
  permission_id text NOT NULL REFERENCES perm.permissions(id) ON DELETE CASCADE,
  granted_at timestamptz NOT NULL DEFAULT now(),
  granted_by text NOT NULL DEFAULT 'system',
  significance boolean NOT NULL DEFAULT false,
  PRIMARY KEY (role_id, permission_id),
  FOREIGN KEY (role_id, role_kind) REFERENCES perm.roles(id, kind) ON DELETE CASCADE
);

CREATE TABLE perm.user_roles (
  user_id integer NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  role_id text NOT NULL,
  role_kind text NOT NULL,
  granted_at timestamptz NOT NULL DEFAULT now(),
  granted_by text,
  PRIMARY KEY (user_id, role_kind),
  FOREIGN KEY (role_id, role_kind) REFERENCES perm.roles(id, kind) ON DELETE RESTRICT
);

CREATE TABLE perm.user_departments (
  user_id integer PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
  department_id text NOT NULL REFERENCES perm.departments(id) ON DELETE RESTRICT,
  assigned_at timestamptz NOT NULL DEFAULT now(),
  assigned_by integer REFERENCES users(id) ON DELETE SET NULL
);

CREATE TABLE perm.department_permissions (
  department_id text NOT NULL REFERENCES perm.departments(id) ON DELETE CASCADE,
  permission_id text NOT NULL REFERENCES perm.permissions(id) ON DELETE CASCADE,
  granted_at timestamptz NOT NULL DEFAULT now(),
  granted_by text NOT NULL DEFAULT 'system',
  significance boolean NOT NULL DEFAULT true,
  PRIMARY KEY (department_id, permission_id)
);

CREATE INDEX perm_user_roles_role_idx ON perm.user_roles(role_id);
CREATE INDEX perm_user_departments_dept_idx ON perm.user_departments(department_id);
CREATE INDEX perm_department_permissions_perm_idx ON perm.department_permissions(permission_id);

INSERT INTO perm.departments (id, display_name) VALUES
  ('development', 'Development'),
  ('marketing', 'Marketing'),
  ('sales', 'Sales'),
  ('law', 'Law'),
  ('hr', 'HR'),
  ('it', 'IT'),
  ('accounting', 'Accounting'),
  ('finance', 'Finance'),
  ('executive', 'Executive');

INSERT INTO perm.roles (id, display_name, description, kind, rank, sort_order) VALUES
  ('staff', 'Staff', 'Entry hierarchy role', 'hierarchy', 7, 70),
  ('officer', 'Officer', 'Operational hierarchy role', 'hierarchy', 6, 60),
  ('supervisor', 'Supervisor', 'First-line department authority', 'hierarchy', 5, 50),
  ('manager', 'Manager', 'Department management authority', 'hierarchy', 4, 40),
  ('director', 'Director', 'Cross-functional leadership authority', 'hierarchy', 3, 30),
  ('cfo', 'CFO', 'Finance executive authority', 'hierarchy', 2, 20),
  ('ceo', 'CEO', 'Chief executive authority', 'hierarchy', 1, 10),
  ('system_admin', 'System Admin', 'Optional administrative bypass role', 'system', NULL, 5);

INSERT INTO perm.permissions (id, description) VALUES
  ('user:dept:accounting::allow', 'Accounting department membership marker'),
  ('rbac:role:edit::allow', 'Create and edit roles'),
  ('rbac:department:assign::allow', 'Assign a department'),
  ('rbac:department:edit::allow', 'Create and edit departments'),
  ('finance:expense:submit::allow', 'Submit an expense claim'),
  ('finance:expense:department_approve::allow', 'Approve a department expense'),
  ('finance:expense:accounting_prepare::allow', 'Prepare expense accrual accounting'),
  ('finance:expense:accounting_approve::allow', 'Approve expense accrual accounting'),
  ('finance:expense:executive_approve::allow', 'Approve a high-value expense'),
  ('finance:expense:pay::allow', 'Pay an approved expense'),
  ('finance:expense:settlement_post::allow', 'Post expense settlement accounting'),
  ('finance:expense:claim::allow', 'Claim an expense-stage task'),
  ('finance:expense:claim_reassign::allow', 'Release or reassign an expense-stage task'),
  ('finance:customer:manage::allow', 'Create, update, and blacklist customers'),
  ('stage:department_approval:act::allow', 'Act at expense department approval'),
  ('stage:accounting_review:act::allow', 'Act at expense accounting review'),
  ('stage:accounting_approval:act::allow', 'Act at expense accounting approval'),
  ('stage:executive_approval:act::allow', 'Act at expense executive approval'),
  ('stage:payment:act::allow', 'Act at expense payment'),
  ('stage:settlement:act::allow', 'Act at expense settlement')
ON CONFLICT (id) DO UPDATE SET description = EXCLUDED.description;

DELETE FROM perm.permissions WHERE id = 'customer:manage::allow';

INSERT INTO perm.permissions (id, description)
SELECT 'user:dept:' || id || '::allow', display_name || ' department membership marker'
  FROM perm.departments
ON CONFLICT (id) DO NOTHING;

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id)
SELECT r.id, r.kind, p.id
  FROM perm.roles r
  JOIN perm.permissions p ON p.id IN (
    'finance:expense:create::allow',
    'finance:expense:submit::allow',
    'finance:expense:update::allow',
    'finance:expense:view_own::allow',
    'tile:expense:view::allow',
    'tile:inbox:view::allow'
  )
 WHERE r.kind = 'hierarchy';

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id)
SELECT r.id, r.kind, p.id
  FROM perm.roles r
  JOIN perm.permissions p ON p.id IN (
    'finance:expense:department_approve::allow',
    'stage:department_approval:act::allow'
  )
 WHERE r.id IN ('supervisor', 'manager', 'director', 'cfo', 'ceo');

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id)
SELECT r.id, r.kind, p.id
  FROM perm.roles r
  JOIN perm.permissions p ON p.id IN (
    'finance:expense:claim_reassign::allow',
    'user:manager:set::allow',
    'user:subtree:edit::allow'
  )
 WHERE r.id IN ('manager', 'director', 'cfo', 'ceo');

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id)
SELECT r.id, r.kind, p.id
  FROM perm.roles r
  JOIN perm.permissions p ON p.id IN (
    'finance:pr:create::allow', 'finance:pr:update::allow',
    'tile:pr:view::allow', 'tile:my_prs:view::allow'
  )
 WHERE r.kind = 'hierarchy';

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id)
SELECT r.id, r.kind, p.id
  FROM perm.roles r
  JOIN perm.permissions p ON p.id IN (
    'finance:pr:approve::allow', 'finance:po:approve::allow',
    'stage:submission:act::allow', 'stage:dept_verification:act::allow',
    'stage:dept_authorization:act::allow', 'stage:po_pending:act::allow',
    'stage:so_dept_approval:act::allow'
  )
 WHERE r.id IN ('supervisor', 'manager', 'director', 'cfo', 'ceo');

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id)
SELECT r.id, r.kind, p.id
  FROM perm.roles r
  JOIN perm.permissions p ON p.id IN (
    'finance:pr:override_approve::allow', 'stage:po_cfo:act::allow',
    'stage:cfo_authorization:act::allow', 'stage:ceo_authorization:act::allow'
  )
 WHERE r.id IN ('cfo', 'ceo');

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id)
SELECT r.id, r.kind, p.id
  FROM perm.roles r
  JOIN perm.permissions p ON p.id IN (
    'finance:expense:executive_approve::allow',
    'finance:expense:override::allow',
    'stage:executive_approval:act::allow'
  )
 WHERE r.id IN ('cfo', 'ceo');

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id)
SELECT 'system_admin', 'system', id
  FROM perm.permissions
 WHERE id = 'admin:system:bypass::allow';

INSERT INTO perm.department_permissions (department_id, permission_id)
SELECT d.id, p.id
  FROM perm.departments d
  JOIN perm.permissions p ON p.id IN ('user:directory:read::allow', 'tile:directory:view::allow');

INSERT INTO perm.department_permissions (department_id, permission_id)
SELECT 'hr', id FROM perm.permissions WHERE id IN (
  'rbac:role:assign::allow', 'rbac:department:assign::allow', 'rbac:role:read::allow',
  'rbac:matrix:view::allow', 'rbac:audit:view::allow', 'user:role:assign::allow',
  'user:dept:edit::allow', 'tile:policy:view::allow', 'tile:roles:view::allow',
  'tile:departments:view::allow', 'tile:audit:view::allow', 'tile:hr:view::allow',
  'tile:hr_employees:view::allow', 'tile:hr_leave:view::allow', 'tile:org_chart:view::allow',
  'user:profile:create::allow', 'user:profile:update::allow',
  'user:profile:deactivate::allow', 'user:profile:delete::allow',
  'hr:analytics:view::allow', 'hr:employee:create::allow',
  'hr:employee:deactivate::allow', 'hr:employee:list::allow',
  'hr:employee:read::allow', 'hr:employee:update::allow',
  'hr:export:csv::allow', 'hr:leave:approve::allow', 'hr:leave:read::allow',
  'hr:leave:reject::allow', 'hr:leave:submit::allow',
  'hr:quota:read::allow', 'hr:quota:update::allow'
);

INSERT INTO perm.department_permissions (department_id, permission_id)
SELECT 'it', id FROM perm.permissions WHERE id IN (
  'rbac:role:assign::allow', 'rbac:department:assign::allow', 'rbac:role:read::allow',
  'rbac:role:edit::allow', 'rbac:department:edit::allow', 'rbac:matrix:view::allow',
  'rbac:matrix:edit::allow', 'rbac:audit:view::allow', 'user:role:assign::allow',
  'user:dept:edit::allow', 'tile:policy:view::allow', 'tile:roles:view::allow',
  'tile:departments:view::allow', 'tile:audit:view::allow', 'tile:settings:view::allow',
  'tile:tile_gates:view::allow', 'tile:dash_it:view::allow'
);

INSERT INTO perm.department_permissions (department_id, permission_id)
SELECT 'accounting', id FROM perm.permissions WHERE id IN (
  'finance:expense:review::allow', 'finance:expense:accounting_prepare::allow',
  'finance:expense:accounting_approve::allow', 'finance:expense:settlement_post::allow',
  'finance:expense:claim::allow', 'finance:gl:view::allow', 'finance:gl:confirm::allow',
  'finance:gl:post::allow', 'finance:ledger:view::allow', 'finance:expense:view_all::allow',
  'stage:accounting_review:act::allow', 'stage:accounting_approval:act::allow',
  'stage:settlement:act::allow', 'tile:ledger:view::allow', 'tile:expense:view::allow',
  'tile:inbox:view::allow', 'tile:pr:view::allow', 'tile:po:view::allow',
  'finance:pr:approve::allow', 'finance:po:approve::allow', 'finance:po:reject::allow',
  'stage:accounting_verification:act::allow', 'stage:accounting_supervision:act::allow',
  'stage:accounting_authorization:act::allow', 'stage:so_credit_check:act::allow',
  'stage:so_invoiced:act::allow', 'finance:sales:invoice::allow'
);

INSERT INTO perm.department_permissions (department_id, permission_id)
SELECT 'finance', id FROM perm.permissions WHERE id IN (
  'finance:expense:pay::allow', 'finance:expense:claim::allow',
  'finance:expense:settle::allow', 'finance:expense:view_all::allow',
  'finance:po:attach_payslip::allow', 'finance:po:settle::allow',
  'stage:payment:act::allow', 'tile:expense:view::allow', 'tile:po:view::allow',
  'tile:inbox:view::allow', 'tile:dash_finance:view::allow',
  'stage:disbursement_authorization:act::allow', 'stage:so_paid:act::allow',
  'finance:customer:manage::allow'
);

INSERT INTO perm.department_permissions (department_id, permission_id)
SELECT 'executive', id FROM perm.permissions WHERE id IN (
  'finance:expense:executive_approve::allow', 'stage:executive_approval:act::allow',
  'finance:report:executive::allow', 'finance:expense:view_all::allow',
  'tile:executive:view::allow', 'tile:dash_exec:view::allow', 'tile:inbox:view::allow',
  'finance:customer:manage::allow'
);

INSERT INTO perm.department_permissions (department_id, permission_id)
SELECT 'sales', id FROM perm.permissions WHERE id IN (
  'finance:sales:submit::allow', 'finance:sales:invoice::allow',
  'finance:sales:settle::allow', 'tile:sales:view::allow', 'tile:inbox:view::allow',
  'stage:so_draft:act::allow', 'stage:so_sales_review:act::allow',
  'finance:customer:manage::allow'
);

INSERT INTO perm.department_permissions (department_id, permission_id)
SELECT 'law', id FROM perm.permissions WHERE id IN (
  'tile:law:view::allow', 'tile:law_admin:view::allow', 'tile:law_upload:view::allow'
);

DELETE FROM perm.user_permissions;

INSERT INTO perm.user_departments (user_id, department_id)
SELECT user_id, department_id FROM access_bootstrap;

INSERT INTO perm.user_roles (user_id, role_id, role_kind, granted_by)
SELECT user_id, hierarchy_role_id, 'hierarchy', 'migration'
  FROM access_bootstrap;

INSERT INTO perm.user_roles (user_id, role_id, role_kind, granted_by)
SELECT user_id, 'system_admin', 'system', 'migration'
  FROM access_bootstrap
 WHERE system_admin;

INSERT INTO perm.user_permissions (user_id, permission_id, granted_by, reason)
SELECT user_id, 'user:dept:' || department_id || '::allow', 'migration', 'Bootstrap department compatibility grant'
  FROM access_bootstrap;

INSERT INTO perm.audit (kind, actor, target)
SELECT 'access_rebuild', 'migration', jsonb_build_object(
  'bootstrapUsers', count(*),
  'departments', 9,
  'hierarchyRoles', 7,
  'systemRoles', 1
)
FROM access_bootstrap;

ALTER TABLE expenses ADD COLUMN IF NOT EXISTS payee_type text NOT NULL DEFAULT 'employee';
ALTER TABLE expenses DROP CONSTRAINT IF EXISTS expenses_payee_type_chk;
ALTER TABLE expenses ADD CONSTRAINT expenses_payee_type_chk CHECK (payee_type IN ('employee', 'vendor'));

ALTER TABLE journal_entries ADD COLUMN IF NOT EXISTS prepared_by integer REFERENCES users(id) ON DELETE SET NULL;
ALTER TABLE journal_entries ADD COLUMN IF NOT EXISTS approved_by integer REFERENCES users(id) ON DELETE SET NULL;
ALTER TABLE journal_entries ADD COLUMN IF NOT EXISTS ai_suggestion jsonb;
ALTER TABLE journal_entries ADD COLUMN IF NOT EXISTS ai_confidence numeric(5,4);

DROP TABLE IF EXISTS waybill_stage_claims;
CREATE TABLE waybill_stage_claims (
  id bigserial PRIMARY KEY,
  waybill_id text NOT NULL REFERENCES waybills(id) ON DELETE RESTRICT,
  stage text NOT NULL,
  claimed_by integer NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
  claimed_at timestamptz NOT NULL DEFAULT now(),
  released_at timestamptz,
  released_by integer REFERENCES users(id) ON DELETE SET NULL,
  release_reason text,
  CHECK (stage IN ('accounting_review', 'payment', 'settlement')),
  CHECK ((released_at IS NULL AND released_by IS NULL) OR released_at IS NOT NULL)
);
CREATE UNIQUE INDEX waybill_stage_claims_one_open ON waybill_stage_claims(waybill_id, stage) WHERE released_at IS NULL;
CREATE INDEX waybill_stage_claims_actor_idx ON waybill_stage_claims(claimed_by) WHERE released_at IS NULL;

DROP TABLE IF EXISTS expense_payments;
CREATE TABLE expense_payments (
  id bigserial PRIMARY KEY,
  waybill_id text NOT NULL UNIQUE REFERENCES waybills(id) ON DELETE RESTRICT,
  expense_id integer NOT NULL UNIQUE REFERENCES expenses(id) ON DELETE RESTRICT,
  slip_id integer NOT NULL UNIQUE REFERENCES slips(id) ON DELETE RESTRICT,
  amount numeric(14,2) NOT NULL CHECK (amount > 0),
  payment_date date NOT NULL,
  bank_name text NOT NULL,
  account_number text,
  payee text NOT NULL,
  reference text NOT NULL,
  ocr_payload jsonb,
  ocr_confidence numeric(5,4),
  confirmed_by integer NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
  confirmed_at timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE slips DROP CONSTRAINT IF EXISTS slips_kind_chk;
ALTER TABLE slips ADD CONSTRAINT slips_kind_chk CHECK (kind IN ('receipt', 'book_bank', 'payment_slip'));

ALTER TABLE waybill_attachments DROP CONSTRAINT IF EXISTS waybill_attachments_kind_check;
ALTER TABLE waybill_attachments ADD CONSTRAINT waybill_attachments_kind_check CHECK (kind IN (
  'slip', 'pr_doc', 'po_doc', 'expense_voucher', 'payment_slip', 'payment_receipt',
  'signoff_memo', 'invoice', 'wht_cert', 'photo', 'memo', 'other'
));

ALTER TABLE waybill_events DROP CONSTRAINT IF EXISTS waybill_events_kind_check;
ALTER TABLE waybill_events ADD CONSTRAINT waybill_events_kind_check CHECK (kind IN (
  'submitted', 'advanced', 'rejected', 'corrected', 'settled', 'posted-to-gl',
  'slip-attached', 'signed-off', 'reversed', 'authorization-overridden', 'resubmitted',
  'superseded', 'created', 'attached', 'gl-confirmed', 'so-submitted', 'so-reviewed',
  'so-credit-checked', 'so-auto-approved', 'so-invoiced', 'so-rejected', 'so-paid',
  'so-dept-approved', 'posted-to-gl-sales-accrual', 'posted-to-gl-sales-vat',
  'posted-to-gl-sales-settlement', 'gl-confirmed-accrual', 'coa-applied',
  'gl-confirmed-settlement', 'gl-confirmed-sales-vat',
  'gl-confirmed-sales-accrual', 'gl-confirmed-sales-settlement',
  'stage-claimed', 'stage-released',
  'stage-reassigned', 'executive-skipped', 'payment-confirmed',
  'posted-to-gl-accrual', 'posted-to-gl-settlement'
));

CREATE TEMP TABLE expense_stage_before AS
SELECT wb.id AS waybill_id, wb.current_stage, COALESCE(wb.total_amount, e.total_amount, 0) AS amount
  FROM waybills wb
  JOIN expenses e ON e.id = wb.origin_id
 WHERE wb.origin = 'expense';

WITH skipped AS (
  SELECT b.waybill_id,
         COALESCE((SELECT max(sequence) FROM waybill_events e WHERE e.waybill_id = b.waybill_id), 0) + 1 AS sequence,
         (SELECT id FROM waybill_events e WHERE e.waybill_id = b.waybill_id ORDER BY sequence DESC LIMIT 1) AS previous_event_id
    FROM expense_stage_before b
   WHERE b.amount <= 200000
     AND b.current_stage IN ('disbursement_authorization', 'cfo_authorization', 'ceo_authorization', 'awaiting_disbursement', 'disbursed')
)
INSERT INTO waybill_events (waybill_id, sequence, previous_event_id, kind, stage_from, stage_to, payload)
SELECT waybill_id, sequence, previous_event_id, 'executive-skipped', 'accounting_approval', 'payment',
       jsonb_build_object('thresholdTHB', 200000, 'reason', 'amount_not_above_threshold')
  FROM skipped;

ALTER TABLE expenses DROP CONSTRAINT IF EXISTS expenses_status_chk;

UPDATE expenses
   SET status = CASE status
     WHEN 'draft' THEN 'draft'
     WHEN 'submission' THEN 'department_approval'
     WHEN 'dept_verification' THEN 'department_approval'
     WHEN 'dept_authorization' THEN 'department_approval'
     WHEN 'accounting_verification' THEN 'accounting_review'
     WHEN 'accounting_supervision' THEN 'accounting_review'
     WHEN 'accounting_authorization' THEN 'accounting_approval'
     WHEN 'disbursement_authorization' THEN CASE WHEN total_amount > 200000 THEN 'executive_approval' ELSE 'payment' END
     WHEN 'cfo_authorization' THEN CASE WHEN total_amount > 200000 THEN 'executive_approval' ELSE 'payment' END
     WHEN 'ceo_authorization' THEN 'payment'
     WHEN 'awaiting_disbursement' THEN 'payment'
     WHEN 'disbursed' THEN CASE WHEN gl_confirmed_at IS NULL THEN 'settlement' ELSE 'completed' END
     WHEN 'rejected' THEN 'rejected'
     ELSE status
   END;

ALTER TABLE expenses ADD CONSTRAINT expenses_status_chk CHECK (status IN (
  'draft', 'submission', 'department_approval', 'accounting_review', 'accounting_approval',
  'executive_approval', 'payment', 'settlement', 'completed', 'rejected'
));

UPDATE waybills wb
   SET current_stage = CASE wb.current_stage
     WHEN 'draft' THEN 'draft'
     WHEN 'submission' THEN 'department_approval'
     WHEN 'dept_verification' THEN 'department_approval'
     WHEN 'dept_authorization' THEN 'department_approval'
     WHEN 'accounting_verification' THEN 'accounting_review'
     WHEN 'accounting_supervision' THEN 'accounting_review'
     WHEN 'accounting_authorization' THEN 'accounting_approval'
     WHEN 'disbursement_authorization' THEN CASE WHEN COALESCE(wb.total_amount, 0) > 200000 THEN 'executive_approval' ELSE 'payment' END
     WHEN 'cfo_authorization' THEN CASE WHEN COALESCE(wb.total_amount, 0) > 200000 THEN 'executive_approval' ELSE 'payment' END
     WHEN 'ceo_authorization' THEN 'payment'
     WHEN 'awaiting_disbursement' THEN 'payment'
     WHEN 'disbursed' THEN CASE WHEN e.gl_confirmed_at IS NULL THEN 'settlement' ELSE 'completed' END
     ELSE wb.current_stage
   END,
       status = CASE
         WHEN wb.current_stage = 'disbursed' AND e.gl_confirmed_at IS NOT NULL THEN 'completed'
         WHEN wb.current_stage = 'rejected' THEN 'rejected'
         ELSE wb.status
       END,
       current_owner_role = NULL,
       current_owner_user_id = NULL,
       updated_at = now()
  FROM expenses e
 WHERE wb.origin = 'expense' AND e.id = wb.origin_id;

COMMIT;
