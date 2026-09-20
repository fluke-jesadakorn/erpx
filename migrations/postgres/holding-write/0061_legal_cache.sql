-- Evidence reads append access logs. These logs are not WorkSnapshot inputs;
-- including them would invalidate every cached task list on each document view.
CREATE OR REPLACE FUNCTION work_cache_tracks(name text) RETURNS boolean LANGUAGE sql IMMUTABLE AS $$
  SELECT name NOT IN ('work_cache_revisions','erpx_migrations','outbox_events',
    'command_assertions','command_receipts','audit_events','runtime_health',
    'iam_audit','iam_receipts','iam_sync_jobs','um_history','seed_repairs',
    'home_read_cursors','active_work_contexts','work_role_preferences',
    'work_card_preferences','navigation_preferences','pos_audit','pms_jobs',
    'legal_access_events','legal_document_inspections','legal_ai_proposals')
    AND name NOT LIKE '%\_receipts' ESCAPE '\'
$$;
DO $$ DECLARE t text; BEGIN
 FOREACH t IN ARRAY ARRAY['legal_access_events','legal_document_inspections','legal_ai_proposals'] LOOP
 EXECUTE format('DROP TRIGGER IF EXISTS work_cache_change ON %I',t);
 EXECUTE format('DROP TRIGGER IF EXISTS work_cache_truncate ON %I',t);
 END LOOP;
END $$;
DELETE FROM work_cache_revisions WHERE table_name IN ('legal_access_events','legal_document_inspections','legal_ai_proposals');
