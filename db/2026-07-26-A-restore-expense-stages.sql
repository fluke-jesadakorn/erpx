BEGIN;

-- ============================================================================
-- 1. Drop the current collapsed CHECK so backfill UPDATEs are accepted.
-- ============================================================================

ALTER TABLE expenses             DROP CONSTRAINT IF EXISTS expenses_status_chk;
ALTER TABLE purchase_requisitions DROP CONSTRAINT IF EXISTS purchase_requisitions_status_chk;
ALTER TABLE purchase_orders      DROP CONSTRAINT IF EXISTS purchase_orders_status_chk;

-- ============================================================================
-- 2. Backfill rows parked at collapsed-only stages.
-- ============================================================================

UPDATE expenses              SET status = 'accounting_authorization' WHERE status = 'accounting_review';
UPDATE purchase_requisitions SET status = 'accounting_authorization' WHERE status = 'accounting_review';
UPDATE purchase_orders       SET status = 'accounting_authorization' WHERE status = 'accounting_review';

UPDATE waybills SET current_stage = 'accounting_authorization' WHERE current_stage = 'accounting_review';

-- ============================================================================
-- 3. Re-add CHECK constraints that include the granular pipeline stages the
--    codebase UI (lib/waybill/labels.ts, components/waybill/*) still renders.
-- ============================================================================

ALTER TABLE expenses ADD CONSTRAINT expenses_status_chk CHECK (status IN (
  'draft','submission','dept_verification','dept_authorization',
  'accounting_verification','accounting_supervision','accounting_authorization',
  'disbursement_authorization','cfo_authorization','ceo_authorization',
  'awaiting_disbursement','disbursed','rejected'
));

ALTER TABLE purchase_requisitions ADD CONSTRAINT purchase_requisitions_status_chk CHECK (status IN (
  'draft','submission','dept_verification','dept_authorization',
  'accounting_verification','accounting_supervision','accounting_authorization',
  'disbursement_authorization','cfo_authorization','ceo_authorization',
  'awaiting_disbursement','disbursed','rejected'
));

ALTER TABLE purchase_orders ADD CONSTRAINT purchase_orders_status_chk CHECK (status IN (
  'draft','submission','dept_verification','dept_authorization',
  'accounting_verification','accounting_supervision','accounting_authorization',
  'disbursement_authorization','cfo_authorization','ceo_authorization',
  'awaiting_disbursement','disbursed','rejected'
));

-- ============================================================================
-- 4. Re-seed the canonical stage permissions removed by 2026-07-24
-- ============================================================================

INSERT INTO perm.permissions (id, description)
VALUES
  ('stage:dept_authorization:act::allow', 'Manager approves the expense on behalf of the submitter''s department'),
  ('stage:accounting_supervision:act::allow', 'Account supervisor reviews the accounting package'),
  ('stage:accounting_authorization:act::allow', 'Accounting manager final-approves the accounting package')
ON CONFLICT (id) DO NOTHING;

-- ============================================================================
-- 5. Grant to the same roles that held them before the collapse
-- ============================================================================

INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT 'manager::3', 'stage:dept_authorization:act::allow', 'migration:2026-07-26-restore-stages'
WHERE NOT EXISTS (
  SELECT 1 FROM perm.role_permissions
   WHERE role_id = 'manager::3' AND permission_id = 'stage:dept_authorization:act::allow'
);

INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT 'account_supervisor::4', 'stage:accounting_supervision:act::allow', 'migration:2026-07-26-restore-stages'
WHERE NOT EXISTS (
  SELECT 1 FROM perm.role_permissions
   WHERE role_id = 'account_supervisor::4' AND permission_id = 'stage:accounting_supervision:act::allow'
);

INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT 'accounting_manager::3', 'stage:accounting_authorization:act::allow', 'migration:2026-07-26-restore-stages'
WHERE NOT EXISTS (
  SELECT 1 FROM perm.role_permissions
   WHERE role_id = 'accounting_manager::3' AND permission_id = 'stage:accounting_authorization:act::allow'
);

-- ============================================================================
-- 6. Grant finance:expense:approve to accounting_manager so the page-level
--    canFinalApprove gate (which only checks that perm) opens for them.
--    Without this, the big-final-approve-{waybillId} button never renders for
--    EMP004 (accounting_manager) and finalApproveWaybillAction rejects them.
-- ============================================================================

INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT 'accounting_manager::3', 'finance:expense:approve::allow', 'migration:2026-07-26-restore-stages'
WHERE NOT EXISTS (
  SELECT 1 FROM perm.role_permissions
   WHERE role_id = 'accounting_manager::3' AND permission_id = 'finance:expense:approve::allow'
);

INSERT INTO perm.audit (kind, actor, target)
VALUES (
  'schema.migration',
  'system',
  jsonb_build_object(
    'migration', '2026-07-26-restore-expense-stages',
    'description', 'Restored dept_authorization / accounting_supervision / accounting_authorization stages and perms removed by 2026-07-24 collapse (UI and lib taxonomy still reference them)'
  )
);

COMMIT;
