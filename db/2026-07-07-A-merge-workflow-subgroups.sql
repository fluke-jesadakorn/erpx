-- 2026-07-07-A: merge Workflow subgroups into a single Workflow group.
--
-- The hub renders three separate headers for what is conceptually one bucket
-- ("Workflow"):
--   - workflow             (Submit Slip, Search, Team Management)
--   - workflow-approval    (Expense Approval)
--   - workflow-procurement (Purchase Requisitions)
--
-- Fold the two subgroups into 'workflow' so all five tiles appear under one
-- "Workflow" header. Display names and sort_order are unchanged.

BEGIN;

UPDATE perm.tiles
   SET group_name = 'workflow'
 WHERE group_name IN ('workflow-approval', 'workflow-procurement');

-- Sanity check (run separately):
--   SELECT group_name, COUNT(*) FROM perm.tiles GROUP BY 1 ORDER BY 1;
-- Expected: workflow=5, finance=3, cockpit=3, hr=3, it=3, policy=1.

COMMIT;
