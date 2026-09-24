CREATE TABLE executive_evidence_access_events (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  assignment_id TEXT NOT NULL,
  provider_id TEXT NOT NULL,
  metric_id TEXT NOT NULL,
  action TEXT NOT NULL CHECK(action IN ('search','open')),
  record_id TEXT,
  record_version BIGINT,
  field_ids JSONB NOT NULL DEFAULT '[]'::jsonb,
  query_hash TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  CHECK((action='search' AND record_id IS NOT NULL AND record_version IS NOT NULL) OR action='open')
);
CREATE INDEX executive_evidence_access_actor ON executive_evidence_access_events(company_id,actor_id,created_at DESC);

CREATE FUNCTION executive_evidence_access_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'EXECUTIVE_EVIDENCE_AUDIT_IMMUTABLE' USING ERRCODE='23514';
END $$;
CREATE TRIGGER executive_evidence_access_no_update BEFORE UPDATE OR DELETE ON executive_evidence_access_events
FOR EACH ROW EXECUTE FUNCTION executive_evidence_access_immutable();
