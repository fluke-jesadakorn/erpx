-- Folio — Grant GL-confirm perm to account personas.
--
-- The new finance:expense:gl_confirm perm lets account_officer /
-- account_supervisor / accounting_manager confirm a posted GL line
-- (4-eye over finance who posted). Finance is intentionally NOT in
-- the grant set.
--
-- Idempotent: re-running is a no-op.

BEGIN;

INSERT INTO perm.permissions (domain, subject, verb, description)
VALUES (
  'finance',
  'expense',
  'gl_confirm',
  'Confirm GL line posted against an expense (account-side, 4-eye over finance)'
)
ON CONFLICT (domain, subject, verb, scope) DO NOTHING;

INSERT INTO perm.role_permissions (role_id, permission_id, effect)
SELECT r.id, 'finance:expense:gl_confirm:all', 'allow'
  FROM perm.roles r
 WHERE r.id IN ('account_officer', 'account_supervisor', 'accounting_manager')
ON CONFLICT DO NOTHING;

COMMIT;
