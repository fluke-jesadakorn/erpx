-- Unknown objects remain intact outside the serving store and canonical quota.
-- This records storage provenance, never guesses a Company or domain owner.
CREATE TABLE attachment_orphan_preservations (
  id TEXT PRIMARY KEY,
  object_key TEXT NOT NULL UNIQUE,
  object_sha256 TEXT NOT NULL CHECK(object_sha256 ~ '^[a-f0-9]{64}$'),
  manifest_sha256 TEXT NOT NULL CHECK(manifest_sha256 ~ '^[a-f0-9]{64}$'),
  bytes BIGINT NOT NULL CHECK(bytes>=0),
  preserved_directory TEXT NOT NULL,
  reference_proof_sha256 TEXT NOT NULL CHECK(reference_proof_sha256 ~ '^[a-f0-9]{64}$'),
  operator_id TEXT NOT NULL,
  reason TEXT NOT NULL,
  created_at BIGINT NOT NULL
);
CREATE TABLE attachment_orphan_preservation_events (
  id TEXT PRIMARY KEY,
  preservation_id TEXT NOT NULL REFERENCES attachment_orphan_preservations(id),
  event_kind TEXT NOT NULL CHECK(event_kind IN ('preserved','restored')),
  operator_id TEXT NOT NULL,
  recorded_at BIGINT NOT NULL,
  UNIQUE(preservation_id,event_kind)
);
CREATE FUNCTION protect_orphan_preservation_history() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'FILE_PRESERVATION_HISTORY_IMMUTABLE' USING ERRCODE='23514'; END;
$$;
CREATE TRIGGER attachment_orphan_preservation_immutable BEFORE UPDATE OR DELETE ON attachment_orphan_preservations
  FOR EACH ROW EXECUTE FUNCTION protect_orphan_preservation_history();
CREATE TRIGGER attachment_orphan_preservation_event_immutable BEFORE UPDATE OR DELETE ON attachment_orphan_preservation_events
  FOR EACH ROW EXECUTE FUNCTION protect_orphan_preservation_history();
DO $$
DECLARE migration_role TEXT := current_user;
DECLARE app_role TEXT;
BEGIN
 IF migration_role !~ '_migration$' THEN RETURN; END IF;
 app_role := regexp_replace(migration_role, '_migration$', '_app');
 EXECUTE format('GRANT SELECT,INSERT ON attachment_orphan_preservations,attachment_orphan_preservation_events TO %I',app_role);
END $$;
