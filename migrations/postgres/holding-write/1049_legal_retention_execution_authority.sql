-- Preserve original approvals and populated legacy journals. Legacy work receives
-- an exact current authorization only through its existing explicit retry/review.
CREATE TABLE legal_retention_execution_authorizations (
 command_id TEXT PRIMARY KEY,
 destruction_id TEXT NOT NULL REFERENCES legal_destructions(id),
 item_id TEXT NOT NULL REFERENCES legal_items(id),
 company_id TEXT NOT NULL REFERENCES organization_nodes(id),
 kind TEXT NOT NULL CHECK(kind IN ('approve','retry')),
 actor_id TEXT NOT NULL,
 assignment_id TEXT NOT NULL,
 authority_json TEXT NOT NULL CHECK(erpx_json_valid(authority_json)),
 source_hash TEXT NOT NULL CHECK(source_hash ~ '^[a-f0-9]{64}$'),
 item_version BIGINT NOT NULL CHECK(item_version>0),
 created_at BIGINT NOT NULL
);
CREATE INDEX legal_retention_execution_authorizations_source ON legal_retention_execution_authorizations(destruction_id,created_at DESC,command_id);
CREATE FUNCTION legal_retention_execution_authorization_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'LEGAL_RETENTION_EXECUTION_AUTHORIZATION_IMMUTABLE' USING ERRCODE='23514';END $$;
CREATE TRIGGER legal_retention_execution_authorization_immutable BEFORE UPDATE OR DELETE ON legal_retention_execution_authorizations FOR EACH ROW EXECUTE FUNCTION legal_retention_execution_authorization_immutable();
