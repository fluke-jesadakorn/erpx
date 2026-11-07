-- Explicit approved sources and atomic complete-observation provenance.
-- No data/observation backfill, extensions, role grants or existing SQL changes.

CREATE UNIQUE INDEX iam_connections_source_company_key ON iam_connections(id,company_id);

CREATE TABLE iam_directory_source_selections (
  connection_id text PRIMARY KEY CHECK(octet_length(connection_id) BETWEEN 1 AND 200),
  company_id text NOT NULL CHECK(octet_length(company_id) BETWEEN 1 AND 200),
  revision bigint NOT NULL CHECK(revision>0),
  format_version integer NOT NULL CHECK(format_version=1),
  key_id text NOT NULL CHECK(key_id ~ '^[A-Za-z0-9_-]{1,80}$'),
  profile_tag bytea NOT NULL CHECK(octet_length(profile_tag)=32),
  connection_version integer NOT NULL CHECK(connection_version>0),
  config_fingerprint bytea NOT NULL CHECK(octet_length(config_fingerprint)=32),
  selected_at bigint NOT NULL CHECK(selected_at>=0),
  UNIQUE(connection_id,company_id),
  FOREIGN KEY(connection_id,company_id) REFERENCES iam_connections(id,company_id)
);

-- A source can be retired by disabling its Connection; deleting its revision
-- history and recreating revision1 would otherwise permit A->B->A revival.
CREATE FUNCTION iam_directory_source_monotonic() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP='DELETE' THEN
    RAISE EXCEPTION 'Directory source revisions must be retained' USING ERRCODE='23514';
  END IF;
  IF TG_OP='INSERT' THEN
    IF NEW.revision<>1 THEN
      RAISE EXCEPTION 'Initial Directory source revision must be one' USING ERRCODE='23514';
    END IF;
  ELSIF NEW.connection_id IS DISTINCT FROM OLD.connection_id
     OR NEW.company_id IS DISTINCT FROM OLD.company_id
     OR NEW.selected_at<OLD.selected_at THEN
    RAISE EXCEPTION 'Directory source revision must advance exactly once' USING ERRCODE='23514';
  END IF;
  IF TG_OP='UPDATE' THEN
    IF OLD.revision=9223372036854775807 THEN
      RAISE EXCEPTION 'Directory source revision exhausted' USING ERRCODE='23514';
    END IF;
    IF NEW.revision<>OLD.revision+1 THEN
      RAISE EXCEPTION 'Directory source revision must advance exactly once' USING ERRCODE='23514';
    END IF;
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER iam_directory_source_monotonic_guard
BEFORE INSERT OR UPDATE OR DELETE ON iam_directory_source_selections
FOR EACH ROW EXECUTE FUNCTION iam_directory_source_monotonic();

CREATE TABLE iam_directory_observation_sources (
  connection_id text PRIMARY KEY CHECK(octet_length(connection_id) BETWEEN 1 AND 200),
  company_id text NOT NULL CHECK(octet_length(company_id) BETWEEN 1 AND 200),
  source_revision bigint NOT NULL CHECK(source_revision>0),
  format_version integer NOT NULL CHECK(format_version=1),
  key_id text NOT NULL CHECK(key_id ~ '^[A-Za-z0-9_-]{1,80}$'),
  profile_tag bytea NOT NULL CHECK(octet_length(profile_tag)=32),
  connection_version integer NOT NULL CHECK(connection_version>0),
  config_fingerprint bytea NOT NULL CHECK(octet_length(config_fingerprint)=32),
  observation_id uuid NOT NULL UNIQUE,
  observed_at bigint NOT NULL CHECK(observed_at>=0),
  FOREIGN KEY(connection_id,company_id)
    REFERENCES iam_directory_source_selections(connection_id,company_id)
);

-- No FK from observation.source_revision to the mutable selected revision:
-- rotation must invalidate an earlier observation, not prevent source rotation.
-- Existing migration-role default privileges apply (see provision.ts); no broad
-- GRANT or SECURITY DEFINER function is introduced by this additive candidate.
