CREATE TABLE social_handover_acknowledgements (
  post_id TEXT NOT NULL REFERENCES social_posts(id),
  user_id TEXT NOT NULL,
  post_version BIGINT NOT NULL CHECK (post_version > 0),
  acknowledged_at BIGINT NOT NULL,
  command_id TEXT NOT NULL UNIQUE,
  PRIMARY KEY (post_id, user_id, post_version)
);
CREATE INDEX social_handover_acknowledgements_version
  ON social_handover_acknowledgements(post_id, post_version);

DO $$
DECLARE migration_role TEXT := current_user; app_role TEXT; jobs_role TEXT;
BEGIN
  IF migration_role !~ '_migration$' THEN RETURN; END IF;
  app_role := regexp_replace(migration_role, '_migration$', '_app');
  jobs_role := regexp_replace(migration_role, '_migration$', '_jobs');
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = app_role)
    OR NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = jobs_role) THEN
    RAISE EXCEPTION 'ERPX_RUNTIME_ROLES_MISSING';
  END IF;
  EXECUTE format('GRANT SELECT,INSERT ON TABLE social_handover_acknowledgements TO %I,%I', app_role, jobs_role);
END $$;
