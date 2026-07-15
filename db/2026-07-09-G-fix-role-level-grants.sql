-- folio/db/2026-07-09-G-fix-role-level-grants.sql
--
-- Migration 0027 (perm/0027_align_levels.sql) updated the legacy
-- perm.roles.level column to the canonical authority bands but never
-- re-issued the rbac:level:grant:min:N:all role_permissions rows.
-- 2026-07-11-B then dropped the column, so the only source of truth
-- for effective_level is the perm grants — and those still reflect
-- the pre-0027 numbers. The sync trigger 0026 added was later
-- dropped by 2026-07-11-C, so this drift is permanent unless
-- patched here.
--
-- Canonical bands (matches lib/roles/display.ts ROLE_LEVEL +
-- perm/0027_align_levels.sql intent):
--   1  ceo
--   2  cfo, admin, finance
--   3  manager, accounting_manager, hr_manager
--   4  supervisor, account_supervisor
--   5  officer, hr, it, account_officer
--
-- User-visible symptom: IT Officer (Alex / Brian in seed 0030) was
-- rendering P2 (Senior Management) on PersonaMenu because the `it`
-- role still held :2:all. After this migration IT Officer = P5
-- (Staff), aligning with the officer < supervisor < manager
-- hierarchy. Same fix drops the stale :4:all duplicate on
-- account_officer (legacy from the accountant collapse) and moves
-- finance from :3:all to :2:all.

BEGIN;

UPDATE perm.role_permissions
   SET permission_id = 'rbac:level:grant:min:5:all',
       granted_by    = 'migration:2026-07-09-G-fix-role-level-grants'
 WHERE role_id = 'it'
   AND permission_id = 'rbac:level:grant:min:2:all';

UPDATE perm.role_permissions
   SET permission_id = 'rbac:level:grant:min:2:all',
       granted_by    = 'migration:2026-07-09-G-fix-role-level-grants'
 WHERE role_id = 'finance'
   AND permission_id = 'rbac:level:grant:min:3:all';

DELETE FROM perm.role_permissions
 WHERE role_id = 'account_officer'
   AND permission_id = 'rbac:level:grant:min:4:all';

INSERT INTO perm.audit (kind, actor, target)
VALUES (
  'schema.migration',
  'system',
  jsonb_build_object(
    'migration', '2026-07-09-G-fix-role-level-grants',
    'it_rewritten', 'rbac:level:grant:min:2:all -> :5:all',
    'finance_rewritten', 'rbac:level:grant:min:3:all -> :2:all',
    'account_officer_dropped', 'rbac:level:grant:min:4:all (stale duplicate from accountant collapse)',
    'rationale', '0027 updated dropped column; sync trigger dropped by 2026-07-11-C; perm grants were stale'
  )
);

COMMIT;
