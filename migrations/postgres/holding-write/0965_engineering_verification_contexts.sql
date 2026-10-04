-- Explicit disposable engineering execution is separate from Company acceptance.
-- Only fixture/migration administrators can enroll or withdraw this context.
CREATE TABLE engineering_verification_contexts (
 company_id TEXT NOT NULL REFERENCES organization_nodes(id),
 run_id TEXT NOT NULL CHECK(run_id ~ '^[a-f0-9]{12}$'),
 source_fingerprint TEXT NOT NULL CHECK(source_fingerprint ~ '^[a-f0-9]{64}$'),
 software_version TEXT NOT NULL CHECK(software_version='release-'||source_fingerprint),
 release_manifest_sha256 TEXT NOT NULL CHECK(release_manifest_sha256 ~ '^[a-f0-9]{64}$'),
 enabled BOOLEAN NOT NULL DEFAULT false,
 expires_at BIGINT NOT NULL CHECK(expires_at>0),
 reason TEXT NOT NULL CHECK(length(reason)>=3),
 created_at BIGINT NOT NULL,
 PRIMARY KEY(company_id,run_id)
);
DO $$ DECLARE runtime_role TEXT; BEGIN
 IF current_user !~ '_migration$' THEN RETURN; END IF;
 FOREACH runtime_role IN ARRAY ARRAY[regexp_replace(current_user,'_migration$','_app'),regexp_replace(current_user,'_migration$','_jobs')] LOOP
  IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname=runtime_role) THEN EXECUTE format('GRANT SELECT ON engineering_verification_contexts TO %I',runtime_role); END IF;
 END LOOP;
END $$;
