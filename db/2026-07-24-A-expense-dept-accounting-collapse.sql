-- folio/db/2026-07-24-A-expense-dept-accounting-collapse.sql
--
-- Collapse expense waybill pipeline:
--   dept_verification + dept_authorization           -> dept_verification (supervisor+manager)
--   accounting_supervision + accounting_authorization -> accounting_review (accounting_manager)
-- Add chart_of_accounts.normal_side for AI COA autofill.
--
-- Idempotent: rerun-safe via WHERE filters, DROP IF EXISTS, ON CONFLICT.

BEGIN;

-- ============================================================================
-- 1. Backfill in-flight waybills + parent rows at removed stages
-- ============================================================================

UPDATE waybills
   SET current_stage = 'dept_verification',
       current_owner_role = 'manager',
       updated_at = now()
 WHERE current_stage = 'dept_authorization';

UPDATE expenses
   SET status = 'dept_verification', updated_at = now()
 WHERE status = 'dept_authorization';

UPDATE purchase_requisitions
   SET status = 'dept_verification', updated_at = now()
 WHERE status = 'dept_authorization';

UPDATE purchase_orders
   SET status = 'dept_verification', updated_at = now()
 WHERE status = 'dept_authorization';

UPDATE waybills
   SET current_stage = 'accounting_review',
       current_owner_role = 'accounting_manager',
       updated_at = now()
 WHERE current_stage IN ('accounting_supervision','accounting_authorization');

UPDATE expenses
   SET status = 'accounting_review', updated_at = now()
 WHERE status IN ('accounting_supervision','accounting_authorization');

UPDATE purchase_requisitions
   SET status = 'accounting_review', updated_at = now()
 WHERE status IN ('accounting_supervision','accounting_authorization');

UPDATE purchase_orders
   SET status = 'accounting_review', updated_at = now()
 WHERE status IN ('accounting_supervision','accounting_authorization');

-- ============================================================================
-- 2. Update status CHECK constraints on expenses / purchase_requisitions / purchase_orders
-- ============================================================================

ALTER TABLE expenses DROP CONSTRAINT IF EXISTS expenses_status_chk;
ALTER TABLE expenses ADD CONSTRAINT expenses_status_chk CHECK (status IN (
  'draft','submission','dept_verification','accounting_verification',
  'accounting_review',
  'disbursement_authorization','cfo_authorization','ceo_authorization',
  'awaiting_disbursement','disbursed','rejected'
));

ALTER TABLE purchase_requisitions DROP CONSTRAINT IF EXISTS purchase_requisitions_status_chk;
ALTER TABLE purchase_requisitions ADD CONSTRAINT purchase_requisitions_status_chk CHECK (status IN (
  'draft','submission','dept_verification','accounting_verification',
  'accounting_review',
  'disbursement_authorization','cfo_authorization','ceo_authorization',
  'awaiting_disbursement','disbursed','rejected'
));

ALTER TABLE purchase_orders DROP CONSTRAINT IF EXISTS purchase_orders_status_chk;
ALTER TABLE purchase_orders ADD CONSTRAINT purchase_orders_status_chk CHECK (status IN (
  'draft','submission','dept_verification','accounting_verification',
  'accounting_review',
  'disbursement_authorization','cfo_authorization','ceo_authorization',
  'awaiting_disbursement','disbursed','rejected'
));

-- ============================================================================
-- 3. Add chart_of_accounts.normal_side + backfill from account_type
-- ============================================================================

ALTER TABLE chart_of_accounts
  ADD COLUMN IF NOT EXISTS normal_side text NOT NULL DEFAULT 'debit';

UPDATE chart_of_accounts SET normal_side = 'debit'
 WHERE account_type IN ('asset','expense');

UPDATE chart_of_accounts SET normal_side = 'credit'
 WHERE account_type IN ('liability','equity','revenue');

ALTER TABLE chart_of_accounts
  DROP CONSTRAINT IF EXISTS chart_of_accounts_normal_side_chk;
ALTER TABLE chart_of_accounts
  ADD CONSTRAINT chart_of_accounts_normal_side_chk
  CHECK (normal_side IN ('debit','credit'));

-- ============================================================================
-- 4. Drop removed stage permissions (cascades to role_permissions + user_permissions)
-- ============================================================================

DELETE FROM perm.permissions
 WHERE id IN (
   'stage:dept_authorization:act::allow',
   'stage:accounting_supervision:act::allow',
   'stage:accounting_authorization:act::allow'
 );

-- ============================================================================
-- 5. Insert new permission + grant for accounting_review
-- ============================================================================

INSERT INTO perm.permissions (id, description)
VALUES ('stage:accounting_review:act::allow', 'Account department approval (combined)')
ON CONFLICT (id) DO NOTHING;

INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, 'stage:accounting_review:act::allow', 'role'
  FROM perm.roles r
 WHERE split_part(r.id, '::', 1) = 'accounting_manager'
ON CONFLICT DO NOTHING;

-- ============================================================================
-- 6. Audit row
-- ============================================================================

INSERT INTO perm.audit (kind, actor, target)
VALUES (
  'schema.migration',
  'system',
  jsonb_build_object(
    'migration', '2026-07-24-A-expense-dept-accounting-collapse',
    'description', 'Collapsed dept_authorization into dept_verification; accounting_supervision+accounting_authorization into accounting_review; added chart_of_accounts.normal_side'
  )
);

COMMIT;
