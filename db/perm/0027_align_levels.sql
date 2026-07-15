-- Folio — Align perm.roles.level with the live seed users + display.ts.
--
-- display.ts ROLE_LEVEL is the canonical source for UI; live users from
-- db/0030_receipt_flow_seed.sql match those values exactly. This migration
-- applies the same authority band to perm.roles so level-perm grants
-- (`rbac:level:grant:min:N:all`) and any join-on-level queries stay in sync.
--
-- Authority bands (lower N = higher authority):
--   1  CEO
--   2  C-Level (cfo, admin, finance)
--   3  Manager (manager, hr_manager, accounting_manager)
--   4  Supervisor (supervisor, account_supervisor)
--   5  Officer   (staff, hr, it, account_officer)

BEGIN;

UPDATE perm.roles SET level = 1 WHERE id = 'ceo';
UPDATE perm.roles SET level = 2 WHERE id IN ('cfo', 'admin', 'finance');
UPDATE perm.roles SET level = 3 WHERE id IN ('manager', 'hr_manager', 'accounting_manager');
UPDATE perm.roles SET level = 4 WHERE id IN ('supervisor', 'account_supervisor');
UPDATE perm.roles SET level = 5 WHERE id IN ('staff', 'hr', 'it', 'account_officer');

UPDATE perm.roles SET level = 5 WHERE kind = 'department';

COMMIT;