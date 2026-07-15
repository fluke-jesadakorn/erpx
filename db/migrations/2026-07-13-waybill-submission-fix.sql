-- folio/db/migrations/2026-07-13-waybill-submission-fix.sql
--
-- Why:
--   Expense waybills were getting current_stage='submission' on submit (and
--   on first insert via ensureWaybillExists), but 'submission' is not a
--   member of the EXPENSE pip set. pipIndex('expense','submission') returns
--   -1, so the page renders every pip as 'pending' and the user with
--   permission sees no approve/reject buttons.
--
--   The event log already records stage_to='dept_verification' for these
--   waybills; this migration aligns waybills.current_stage with the event
--   trail. Procurement waybills keep 'submission' as the first approval
--   step (no change).
--
-- Forward:
UPDATE waybills
   SET current_stage = 'dept_verification',
       updated_at = now()
 WHERE origin = 'expense'
   AND current_stage = 'submission'
   AND status = 'open';

-- Rollback (run in reverse if needed):
-- UPDATE waybills
--    SET current_stage = 'submission',
--        updated_at = now()
--  WHERE origin = 'expense'
--    AND current_stage = 'dept_verification'
--    AND status = 'open'
--    AND id IN (SELECT waybill_id FROM waybill_events WHERE stage_to = 'dept_verification');
