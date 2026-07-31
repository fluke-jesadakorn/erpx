-- Durable directory RBAC projection and database-level source invariants.
-- The projection is disposable; role facts and their outbox events remain the
-- authoritative source and can rebuild it after a read-side loss.
ALTER TABLE erpx_identity.directory_outbox
    ADD COLUMN IF NOT EXISTS projected_at TIMESTAMPTZ;

ALTER TABLE erpx_identity.directory_outbox
    ADD COLUMN IF NOT EXISTS projection_attempts INTEGER NOT NULL DEFAULT 0;

ALTER TABLE erpx_identity.directory_outbox
    DROP CONSTRAINT IF EXISTS directory_outbox_projection_attempts_check;
ALTER TABLE erpx_identity.directory_outbox
    ADD CONSTRAINT directory_outbox_projection_attempts_check
    CHECK (projection_attempts >= 0);

UPDATE erpx_identity.directory_outbox
SET idempotency_key = event_id::text
WHERE idempotency_key IS NULL;

ALTER TABLE erpx_identity.directory_outbox
    ALTER COLUMN idempotency_key SET NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS directory_outbox_connector_idempotency_idx
    ON erpx_identity.directory_outbox (connector_id, idempotency_key)
    WHERE connector_id IS NOT NULL;

ALTER TABLE erpx_identity.directory_role_facts
    DROP CONSTRAINT IF EXISTS directory_role_facts_sequence_check;
ALTER TABLE erpx_identity.directory_role_facts
    ADD CONSTRAINT directory_role_facts_sequence_check
    CHECK (sequence > 0);

CREATE UNIQUE INDEX IF NOT EXISTS directory_role_facts_chain_sequence_idx
    ON erpx_identity.directory_role_facts
       (connector_id, identity_link, role_ref, scope_key, sequence);

CREATE OR REPLACE FUNCTION erpx_identity.reject_directory_role_fact_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'directory role facts are append-only';
END;
$$;

DROP TRIGGER IF EXISTS directory_role_facts_append_only ON erpx_identity.directory_role_facts;
CREATE TRIGGER directory_role_facts_append_only
    BEFORE UPDATE OR DELETE ON erpx_identity.directory_role_facts
    FOR EACH ROW
    EXECUTE FUNCTION erpx_identity.reject_directory_role_fact_mutation();

CREATE TABLE IF NOT EXISTS erpx_identity.directory_role_read_model (
    connector_id UUID NOT NULL REFERENCES erpx_identity.directory_connectors(id),
    identity_link TEXT NOT NULL,
    role_ref TEXT NOT NULL,
    scope_key TEXT NOT NULL,
    group_ref TEXT NOT NULL,
    active BOOLEAN NOT NULL,
    last_sequence BIGINT NOT NULL CHECK (last_sequence > 0),
    last_fact_hash TEXT NOT NULL,
    last_event_id UUID NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (connector_id, identity_link, role_ref, scope_key)
);

CREATE INDEX IF NOT EXISTS directory_role_read_model_active_idx
    ON erpx_identity.directory_role_read_model
       (connector_id, identity_link, active, role_ref, scope_key);

CREATE INDEX IF NOT EXISTS directory_outbox_role_projection_pending_idx
    ON erpx_identity.directory_outbox
       (occurred_at, aggregate_id, aggregate_version, event_id)
    WHERE aggregate_type = 'directory_role' AND projected_at IS NULL;
