-- Folio — YouTube cast cull
-- Run after `pg_dump folio_db > /tmp/pre-youtube-cull.sql`.
--
-- Trims the demo persona + department footprint down to 6 users / 4 dept groups
-- so each YouTube episode can focus on one flow without cognitive overhead.
--
-- Reassigns every FK reference to the closest kept equivalent BEFORE delete
-- (NO ACTION FKs would otherwise block DELETE).

BEGIN;

-- 1. Keep chains consistent: John's supervisor was Steven (being removed).
UPDATE users SET reports_to_user_id = 27 WHERE id = 1;
-- Any other reports_to pointing at removed users → null
UPDATE users SET reports_to_user_id = NULL
 WHERE reports_to_user_id IN (2,3,5,16,17,18,19,21,22,24,25,26);

-- 2. Reassign approval_logs to Andrew (same finance team / L2A peer)
UPDATE approval_logs SET actor_id = 27
 WHERE actor_id IN (2,3,18,19);

-- 3. Reassign slips to Charles (same executive dept)
UPDATE slips SET uploaded_by = 15 WHERE uploaded_by = 5;

-- 4. Reassign expenses.submitter for Karen (only 1 expense) → John (L2A peer)
UPDATE expenses SET submitter_id = 1 WHERE submitter_id = 19;

-- 5. Clear approval_policies.created_by (audit only)
UPDATE approval_policies SET created_by = NULL
 WHERE created_by IN (2,3,5,16,17,18,19,21,22,24,25,26);

-- 6. Clear departments.head_user_id for depts we're dropping entirely
UPDATE departments SET head_user_id = NULL
 WHERE head_user_id IN (2,3,5,16,17,18,19,21,22,24,25,26);

-- 7. Drop the access request whose target_user_id is a removed HR manager
UPDATE access_requests SET target_user_id = NULL WHERE target_user_id = 24;

-- 8. Drop dept_group_id from removed users (no users remain after step 9 but
--    the trigger may not fire on cascade delete — be explicit)
UPDATE users SET dept_group_id = NULL WHERE id IN (2,3,5,16,17,18,19,21,22,24,25,26);

-- 9. Delete removed users. CASCADE handles:
--    - notifications (CASCADE)
--    - domain_events (SET NULL)
--    - ai_invocations (SET NULL)
--    All other NO ACTION FKs have been pre-cleared above.
DELETE FROM users WHERE id IN (2,3,5,16,17,18,19,21,22,24,25,26);

-- 10. Drop dept groups with no remaining users (Engineering, Sales, Marketing,
--     Operations, Human Resource) plus two orphans from earlier rename
--     (dept-finance, dept-hr had no users).
DELETE FROM rbac.groups WHERE id IN (
  'dept-engineering',
  'dept-sales',
  'dept-marketing',
  'dept-operations',
  'dept-hr-2',
  'dept-finance',
  'dept-hr'
);

-- 11. Drop the corresponding legacy `departments` rows
DELETE FROM departments WHERE name IN (
  'Engineering',
  'Sales',
  'Marketing',
  'Operations',
  'Human Resource'
);

COMMIT;