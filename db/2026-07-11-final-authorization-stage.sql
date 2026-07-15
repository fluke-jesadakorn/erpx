-- folio/db/2026-07-11-final-authorization-stage.sql
--
-- Add a new 'final_authorization' stage to the Waybill pipeline between
-- 'accounting_authorization' and 'awaiting_disbursement'.
--
-- It is the gatekeeper stage for GL posting:
--   * finalApproveWaybillAction  -> posts to GL + advances to awaiting_disbursement
--   * finalRejectWaybillAction   -> moves to 'rejected', no GL post
--
-- Access: same set as the existing disbursement step (admin/ceo/cfo/finance/it).
-- Audit logic: append a 'posted-to-gl' waybill_event when final-approve fires.

BEGIN;

-- ============================================================================
-- 1. Extend status CHECK constraints to accept 'final_authorization'
-- ============================================================================

ALTER TABLE expenses DROP CONSTRAINT IF EXISTS expenses_status_chk;
ALTER TABLE expenses ADD CONSTRAINT expenses_status_chk CHECK (status IN (
  'draft','submission','dept_verification','dept_authorization',
  'accounting_verification','accounting_supervision','accounting_authorization',
  'final_authorization',
  'disbursement_authorization','cfo_authorization','ceo_authorization',
  'awaiting_disbursement','disbursed','rejected'
));

ALTER TABLE purchase_requisitions DROP CONSTRAINT IF EXISTS purchase_requisitions_status_chk;
ALTER TABLE purchase_requisitions ADD CONSTRAINT purchase_requisitions_status_chk CHECK (status IN (
  'draft','submission','dept_verification','dept_authorization',
  'accounting_verification','accounting_supervision','accounting_authorization',
  'final_authorization',
  'disbursement_authorization','cfo_authorization','ceo_authorization',
  'awaiting_disbursement','disbursed','rejected'
));

ALTER TABLE purchase_orders DROP CONSTRAINT IF EXISTS purchase_orders_status_chk;
ALTER TABLE purchase_orders ADD CONSTRAINT purchase_orders_status_chk CHECK (status IN (
  'draft','submission','dept_verification','dept_authorization',
  'accounting_verification','accounting_supervision','accounting_authorization',
  'final_authorization',
  'disbursement_authorization','cfo_authorization','ceo_authorization',
  'awaiting_disbursement','disbursed','rejected'
));

-- ============================================================================
-- 2. Seed the canonical permission
-- ============================================================================

INSERT INTO perm.permissions (domain, subject, verb, scope, description) VALUES
  ('stage', 'final_authorization', 'act', 'all', 'Finance final approval gate (posts to GL on approve)'),
  ('stage', 'final_authorization', 'act', 'dept', 'Finance final approval gate (own dept)')
ON CONFLICT DO NOTHING;

-- ============================================================================
-- 3. Grant to the same set as the disbursement step
-- ============================================================================

INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT rp.role_id, 'stage:final_authorization:act:all', rp.effect, 'migration:2026-07-11-final-auth'
  FROM perm.role_permissions rp
 WHERE rp.permission_id = 'stage:disbursement_authorization:act:all'
ON CONFLICT (role_id, permission_id) DO NOTHING;

INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT rp.role_id, 'stage:final_authorization:act:dept', rp.effect, 'migration:2026-07-11-final-auth'
  FROM perm.role_permissions rp
 WHERE rp.permission_id = 'stage:disbursement_authorization:act:dept'
ON CONFLICT (role_id, permission_id) DO NOTHING;

-- ============================================================================
-- 4. Audit row
-- ============================================================================

INSERT INTO perm.audit (kind, actor, target)
VALUES (
  'schema.migration',
  'system',
  jsonb_build_object(
    'migration', '2026-07-11-final-authorization-stage',
    'description', 'Added final_authorization stage as the GL-posting gatekeeper between accounting_authorization and awaiting_disbursement',
    'new_stage_key', 'final_authorization',
    'new_permission', 'stage:final_authorization:act:all'
  )
);

COMMIT;
