DO $$
DECLARE migration_role TEXT := current_user; app_role TEXT; jobs_role TEXT;
BEGIN
 IF migration_role !~ '_migration$' THEN RETURN; END IF;
 app_role := regexp_replace(migration_role,'_migration$','_app');
 jobs_role := regexp_replace(migration_role,'_migration$','_jobs');
 IF NOT EXISTS(SELECT 1 FROM pg_roles WHERE rolname=app_role)
    OR NOT EXISTS(SELECT 1 FROM pg_roles WHERE rolname=jobs_role) THEN
  RAISE EXCEPTION 'ERPX_RUNTIME_ROLES_MISSING';
 END IF;
 EXECUTE format('GRANT SELECT,INSERT,UPDATE,DELETE ON TABLE inbox_events,inbox_deliveries,inbox_notices,inbox_notice_targets,calendar_attendees,calendar_series,calendar_occurrence_exceptions,calendar_occurrence_responses,calendar_reminder_deliveries TO %I,%I',app_role,jobs_role);
END $$;

INSERT INTO iam_permissions(id,definition)
VALUES('inbox.notice.broadcast',jsonb_build_object('id','inbox.notice.broadcast','label','Broadcast a staff notice','module','collaboration','namespace','customer','availability','live','risk','privileged','roles','["holding_owner","hr_admin","support_manager","quality_manager","legal_manager","pms_manager","pos_manager","people_manager","sales_manager","purchasing_manager","warehouse_manager","operations_manager"]'::jsonb,'conditions','["Current assignment scope"]'::jsonb,'permittedScopes','["holding","branch","department"]'::jsonb))
ON CONFLICT(id) DO NOTHING;
INSERT INTO iam_role_revisions
 SELECT r.company_id,r.id,r.published_revision+1,
 v.permissions_json || '["inbox.notice.broadcast"]'::jsonb,
 v.grantable_json || CASE WHEN r.id IN ('holding_owner','hr_admin') THEN '["inbox.notice.broadcast"]'::jsonb ELSE '[]'::jsonb END,
 v.scopes_json,'system',floor(extract(epoch FROM clock_timestamp())*1000)::bigint
 FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision
 WHERE r.company_id='*' AND r.id IN ('holding_owner','hr_admin','support_manager','quality_manager','legal_manager','pms_manager','pos_manager','people_manager','sales_manager','purchasing_manager','warehouse_manager','operations_manager')
 AND NOT v.permissions_json @> '["inbox.notice.broadcast"]'::jsonb;
UPDATE iam_roles r SET published_revision=(SELECT max(revision) FROM iam_role_revisions v WHERE v.company_id=r.company_id AND v.role_id=r.id)
 WHERE r.company_id='*' AND r.id IN ('holding_owner','hr_admin','support_manager','quality_manager','legal_manager','pms_manager','pos_manager','people_manager','sales_manager','purchasing_manager','warehouse_manager','operations_manager');
