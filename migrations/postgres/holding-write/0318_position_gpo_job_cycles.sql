-- Completed jobs are historical evidence. A later unlink/relink cycle may
-- enqueue a fresh job for the same account and unchanged Position revision.
DROP INDEX IF EXISTS position_gpo_account_jobs_unique;
CREATE UNIQUE INDEX position_gpo_account_jobs_active ON position_gpo_jobs
 (company_id,engagement_id,account_id,connection_id,filter_group_id,revision,desired_action)
 WHERE engagement_id IS NOT NULL AND account_id IS NOT NULL
   AND status IN ('queued','running','retry','blocked');
