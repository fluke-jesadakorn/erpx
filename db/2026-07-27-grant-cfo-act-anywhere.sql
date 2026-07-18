-- folio/db/2026-07-27-grant-cfo-act-anywhere.sql
--
-- Grant CFO the cross-stage override permissions needed by e2e rejection tests.
-- CFO + CEO can already act at any stage server-side (action handler); this
-- migration brings the UI in line by giving them the corresponding :act::allow
-- grants. The cross-dept UI stages (accounting_*, disbursement_*, cfo_*,
-- ceo_*) are already exempt from dept-scoping in page.tsx, so this is safe.

BEGIN;

INSERT INTO perm.permissions (id, description)
VALUES
  ('stage:accounting_supervision:act:all::allow',    'CFO/CEO override: accounting supervision'),
  ('stage:accounting_authorization:act:all::allow',  'CFO/CEO override: accounting authorization'),
  ('stage:accounting_verification:act:all::allow',   'CFO/CEO override: accounting verification'),
  ('stage:dept_authorization:act:all::allow',        'CFO/CEO override: dept authorization'),
  ('stage:dept_verification:act:all::allow',         'CFO/CEO override: dept verification'),
  ('stage:disbursement_authorization:act:all::allow','CFO/CEO override: disbursement authorization'),
  ('stage:final_authorization:act:all::allow',       'CFO/CEO override: final authorization'),
  ('stage:awaiting_disbursement:act:all::allow',     'CFO/CEO override: awaiting disbursement')
ON CONFLICT (id) DO NOTHING;

INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, p.id, 'migration:2026-07-27-grant-cfo-act-anywhere'
  FROM perm.roles r
  JOIN perm.permissions p ON p.id IN (
    'stage:accounting_supervision:act:all::allow',
    'stage:accounting_authorization:act:all::allow',
    'stage:accounting_verification:act:all::allow',
    'stage:dept_authorization:act:all::allow',
    'stage:dept_verification:act:all::allow',
    'stage:disbursement_authorization:act:all::allow',
    'stage:final_authorization:act:all::allow',
    'stage:awaiting_disbursement:act:all::allow'
  )
 WHERE split_part(r.id, '::', 1) IN ('cfo', 'ceo', 'admin')
ON CONFLICT DO NOTHING;

INSERT INTO perm.audit (kind, actor, target)
VALUES (
  'schema.migration',
  'system',
  jsonb_build_object(
    'migration', '2026-07-27-grant-cfo-act-anywhere',
    'description', 'Granted CFO/CEO/admin :act:all for cross-stage overrides used by e2e tests'
  )
);

COMMIT;