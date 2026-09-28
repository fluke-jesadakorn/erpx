-- Account-specific delivery and ownership of only the Directory configuration
-- created by ERPX. Existing Directory values are intentionally not backfilled:
-- their provenance cannot be established from the old policy table.
ALTER TABLE position_gpo_jobs ADD COLUMN engagement_id text;
ALTER TABLE position_gpo_jobs ADD COLUMN account_id text;
ALTER TABLE position_gpo_jobs DROP CONSTRAINT position_gpo_jobs_status_check;
ALTER TABLE position_gpo_jobs ADD CONSTRAINT position_gpo_jobs_status_check
 CHECK(status IN ('queued','awaiting_identity','running','retry','directory_applied','blocked','superseded'));

DO $$ DECLARE constraint_name text; BEGIN
 SELECT c.conname INTO constraint_name FROM pg_constraint c
 WHERE c.conrelid='position_gpo_jobs'::regclass AND c.contype='u'
 AND pg_get_constraintdef(c.oid) LIKE '%(company_id, position_id, person_id, revision, engagement_version, desired_action)%';
 IF constraint_name IS NOT NULL THEN
  EXECUTE format('ALTER TABLE position_gpo_jobs DROP CONSTRAINT %I',constraint_name);
 END IF;
END $$;
CREATE UNIQUE INDEX position_gpo_account_jobs_unique ON position_gpo_jobs
 (company_id,position_id,engagement_id,account_id,revision,engagement_version,desired_action)
 WHERE engagement_id IS NOT NULL AND account_id IS NOT NULL;
CREATE INDEX position_gpo_account_claims_lookup ON people_directory_membership_sources(source_id)
 WHERE source_id LIKE 'position-gpo:%';

CREATE TABLE position_gpo_config_ownership (
 connection_id text NOT NULL REFERENCES iam_connections(id),
 gpo_id text NOT NULL,
 kind text NOT NULL CHECK(kind IN ('filter_group','ou_link')),
 target_id text NOT NULL,
 created_at bigint NOT NULL,
 PRIMARY KEY(connection_id,gpo_id,kind,target_id)
);

DO $$ DECLARE migration_role text := current_user; app_role text; jobs_role text; BEGIN
 IF migration_role !~ '_migration$' THEN RETURN; END IF;
 app_role := regexp_replace(migration_role,'_migration$','_app');
 jobs_role := regexp_replace(migration_role,'_migration$','_jobs');
 EXECUTE format('GRANT SELECT,INSERT,UPDATE,DELETE ON position_gpo_config_ownership TO %I,%I',app_role,jobs_role);
END $$;
