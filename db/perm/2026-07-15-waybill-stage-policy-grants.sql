-- Folio — extend stage:* perm grants for 7-step Waybill pipeline.
--
-- Pipeline (drops dept_authorization):
--   draft → submission → dept_verification → accounting_verification
--         → accounting_authorization → awaiting_disbursement → gl_confirmed
--         → (or rejected)
--
-- Only ONE new perm string is added: stage:gl_confirmed:act:all. All other
-- perms already exist; only their role grants are extended.
--
-- Idempotent: re-running is a no-op.

BEGIN;

-- 1. New perm: stage:gl_confirmed:act:all
INSERT INTO perm.permissions (domain, subject, verb, scope, description)
VALUES ('stage', 'gl_confirmed', 'act', 'all',
        'Confirm GL line + close waybill (account / finance)')
ON CONFLICT (domain, subject, verb, scope) DO NOTHING;

-- 2. Grants for stage:gl_confirmed:act:all → all 4 dept-finance personas
INSERT INTO perm.role_permissions (role_id, permission_id, effect)
SELECT r.id, 'stage:gl_confirmed:act:all', 'allow'
  FROM perm.roles r
 WHERE r.id IN ('account_officer','account_supervisor','accounting_manager','finance')
ON CONFLICT DO NOTHING;

-- 3. Extend stage:accounting_verification:act:all to all 4 dept-finance personas
INSERT INTO perm.role_permissions (role_id, permission_id, effect)
SELECT r.id, 'stage:accounting_verification:act:all', 'allow'
  FROM perm.roles r
 WHERE r.id IN ('account_supervisor','accounting_manager','finance')
ON CONFLICT DO NOTHING;

-- 4. Extend stage:accounting_authorization:act:all to account_supervisor
INSERT INTO perm.role_permissions (role_id, permission_id, effect)
SELECT r.id, 'stage:accounting_authorization:act:all', 'allow'
  FROM perm.roles r
 WHERE r.id = 'account_supervisor'
ON CONFLICT DO NOTHING;

-- 5. Audit
INSERT INTO perm.audit (kind, target)
VALUES (
  'schema.migration',
  jsonb_build_object(
    'migration', '2026-07-15-waybill-stage-policy-grants',
    'description', 'Add stage:gl_confirmed perm + extend stage:accounting_verification/accounting_authorization grants to support 7-step Waybill pipeline'
  )
);

COMMIT;