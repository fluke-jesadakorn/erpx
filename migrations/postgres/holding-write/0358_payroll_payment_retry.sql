-- A rejected bank row stays unpaid. Each corrected instruction is a new,
-- independently approved export while the original evidence remains retained.
ALTER TABLE hr_payment_batches ADD COLUMN retry_requested_by TEXT;

CREATE TABLE hr_payment_batch_exports (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  batch_id TEXT NOT NULL REFERENCES hr_payment_batches(id),
  batch_version BIGINT NOT NULL,
  file_key TEXT NOT NULL,
  checksum_sha256 TEXT NOT NULL,
  item_count BIGINT NOT NULL CHECK (item_count > 0),
  total_minor BIGINT NOT NULL CHECK (total_minor > 0),
  exported_by TEXT NOT NULL,
  exported_at BIGINT NOT NULL,
  UNIQUE(batch_id,batch_version),
  UNIQUE(file_key)
);
CREATE INDEX hr_payment_batch_exports_batch ON hr_payment_batch_exports(batch_id,exported_at DESC);

DO $$
DECLARE migration_role text := current_user;
DECLARE app_role text;
DECLARE jobs_role text;
BEGIN
  IF migration_role !~ '_migration$' THEN RETURN; END IF;
  app_role := regexp_replace(migration_role, '_migration$', '_app');
  jobs_role := regexp_replace(migration_role, '_migration$', '_jobs');
  EXECUTE format('GRANT SELECT, INSERT, UPDATE ON hr_payment_batch_exports TO %I', app_role);
  EXECUTE format('GRANT SELECT ON hr_payment_batch_exports TO %I', jobs_role);
END $$;
