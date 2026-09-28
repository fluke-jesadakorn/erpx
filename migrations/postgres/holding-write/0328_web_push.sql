-- Web Push is a delivery channel for existing Inbox rows, never a new source event.
CREATE TABLE push_preferences (
 company_id TEXT NOT NULL, user_id TEXT NOT NULL,
 enabled BOOLEAN NOT NULL DEFAULT false,
 work_enabled BOOLEAN NOT NULL DEFAULT true,
 mention_enabled BOOLEAN NOT NULL DEFAULT true,
 system_enabled BOOLEAN NOT NULL DEFAULT true,
 calendar_enabled BOOLEAN NOT NULL DEFAULT false,
 notice_enabled BOOLEAN NOT NULL DEFAULT false,
 quiet_start_minute INTEGER CHECK(quiet_start_minute BETWEEN 0 AND 1439),
 quiet_end_minute INTEGER CHECK(quiet_end_minute BETWEEN 0 AND 1439),
 updated_at BIGINT NOT NULL,
 PRIMARY KEY(company_id,user_id),
 CHECK((quiet_start_minute IS NULL)=(quiet_end_minute IS NULL)),
 CHECK(quiet_start_minute IS NULL OR quiet_start_minute<>quiet_end_minute)
);
CREATE TABLE push_subscriptions (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, user_id TEXT NOT NULL,
 session_id TEXT NOT NULL, origin TEXT NOT NULL, inbox_path TEXT NOT NULL,
 endpoint_hash TEXT NOT NULL, sealed_subscription TEXT NOT NULL,
 created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
 scan_at BIGINT NOT NULL, cursor_at BIGINT NOT NULL, cursor_id TEXT NOT NULL DEFAULT '',
 UNIQUE(company_id,endpoint_hash),
 FOREIGN KEY(company_id,user_id) REFERENCES push_preferences(company_id,user_id) ON DELETE CASCADE
);
CREATE INDEX push_subscriptions_scan ON push_subscriptions(scan_at,id);
CREATE INDEX push_subscriptions_session ON push_subscriptions(company_id,user_id,session_id);
CREATE TABLE push_delivery_jobs (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL,
 inbox_delivery_id TEXT NOT NULL REFERENCES inbox_deliveries(id) ON DELETE CASCADE,
 subscription_id TEXT NOT NULL REFERENCES push_subscriptions(id) ON DELETE CASCADE,
 status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','sent','dead')),
 attempts INTEGER NOT NULL DEFAULT 0, next_attempt_at BIGINT NOT NULL,
 lease_token TEXT, lease_until BIGINT, last_status INTEGER,
 created_at BIGINT NOT NULL, sent_at BIGINT,
 UNIQUE(inbox_delivery_id,subscription_id)
);
CREATE INDEX push_delivery_due ON push_delivery_jobs(next_attempt_at,id)
 WHERE status='pending';
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
 EXECUTE format('GRANT SELECT,INSERT,UPDATE,DELETE ON TABLE push_preferences,push_subscriptions,push_delivery_jobs TO %I,%I',app_role,jobs_role);
END $$;
