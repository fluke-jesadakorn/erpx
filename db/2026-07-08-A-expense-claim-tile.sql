-- 2026-07-08-A-expense-claim-tile.sql
-- Rename the "Submit Slip" tile + URL to "Expense Claim" / /expense-claim
-- so the user-facing entry point for reimbursement matches the new language
-- (User Claim / My Claim / Claim #EXP-...).
--
-- Also register the tile permission `tile:expense_claim:view` if missing so the
-- StageChain + /policy matrix keeps seeing the same view-perm name.
--
-- Additive: never drops data. If the old tile row was already renamed by a
-- prior run the UPDATE becomes a no-op.

BEGIN;

UPDATE perm.tiles
   SET id          = 'expense-claim',
       display_name= 'Expense Claim',
       subtitle    = 'Submit & track reimbursements',
       icon        = '🧾',
       accent      = 'emerald',
       group_name  = 'workflow',
       sub_view    = 'submit',
       href        = '/expense-claim',
       request_target = 'hr_manager',
       sort_order  = 100
 WHERE id = 'submit-expense';

INSERT INTO perm.tiles
  (id, display_name, subtitle, icon, accent, group_name, sub_view, href, request_target, sort_order)
SELECT
  'expense-claim', 'Expense Claim', 'Submit & track reimbursements',
  '🧾', 'emerald', 'workflow', 'submit', '/expense-claim',
  'hr_manager', 100
WHERE NOT EXISTS (
  SELECT 1 FROM perm.tiles WHERE id = 'expense-claim'
);

INSERT INTO perm.permissions (domain, subject, verb, description)
  SELECT 'tile', 'expense_claim', 'view', 'Open Expense Claim tile'
  WHERE NOT EXISTS (
    SELECT 1 FROM perm.permissions
     WHERE domain = 'tile' AND subject = 'expense_claim' AND verb = 'view'
  );

INSERT INTO perm.audit (kind, actor, target)
VALUES
  ('tile.rename', 'migration-2026-07-08',
   jsonb_build_object('from','submit-expense','to','expense-claim','href','/expense-claim'));

COMMIT;
