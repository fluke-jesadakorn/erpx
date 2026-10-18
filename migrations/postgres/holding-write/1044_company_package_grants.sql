-- Additive, Holding-authoritative commercial history. No assignments or rollout changes.
CREATE TABLE company_package_grants (
  company_id text NOT NULL,
  package_id text NOT NULL,
  package_version integer NOT NULL CHECK (package_version > 0),
  state text NOT NULL CHECK (state IN ('active','draining','inactive')),
  starts_at bigint NOT NULL CHECK (starts_at >= 0),
  ends_at bigint CHECK (ends_at > starts_at),
  version integer NOT NULL CHECK (version > 0),
  reason text NOT NULL CHECK (length(btrim(reason)) > 0),
  evidence text NOT NULL CHECK (length(btrim(evidence)) > 0),
  updated_at bigint NOT NULL,
  PRIMARY KEY (company_id,package_id)
);
CREATE TABLE company_package_grant_events (
  id text PRIMARY KEY,
  company_id text NOT NULL,
  package_id text NOT NULL,
  actor_id text NOT NULL,
  request_hash text NOT NULL,
  before_json jsonb,
  after_json jsonb NOT NULL,
  created_at bigint NOT NULL
);
CREATE INDEX company_package_grant_events_company ON company_package_grant_events(company_id,created_at,id);
