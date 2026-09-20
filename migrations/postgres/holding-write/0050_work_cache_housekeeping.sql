-- Business job exceptions (pos_jobs) remain tracked. Audit output and PMS worker
-- coordination do not participate in WorkSnapshot; pms_records owns its tasks.
CREATE OR REPLACE FUNCTION work_cache_tracks(name text) RETURNS boolean LANGUAGE sql IMMUTABLE AS $$
  SELECT name NOT IN ('work_cache_revisions','erpx_migrations','outbox_events',
    'command_assertions','command_receipts','audit_events','runtime_health',
    'iam_audit','iam_receipts','iam_sync_jobs','um_history','seed_repairs',
    'home_read_cursors','active_work_contexts','work_role_preferences',
    'work_card_preferences','navigation_preferences','pos_audit','pms_jobs')
    AND name NOT LIKE '%\_receipts' ESCAPE '\'
$$;
DROP TRIGGER work_cache_change ON pos_audit;
DROP TRIGGER work_cache_truncate ON pos_audit;
DROP TRIGGER work_cache_change ON pms_jobs;
DROP TRIGGER work_cache_truncate ON pms_jobs;
DELETE FROM work_cache_revisions WHERE table_name IN ('pos_audit','pms_jobs');
