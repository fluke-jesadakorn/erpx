-- Authoritative tenant-write database generation marker.
ALTER TABLE erpx_cqrs_write.outbox_events
    ADD COLUMN claim_token UUID,
    ADD COLUMN claim_until TIMESTAMPTZ,
    ADD COLUMN dead_lettered_at TIMESTAMPTZ,
    ADD CONSTRAINT cqrs_tenant_outbox_claim_consistency
        CHECK ((claim_token IS NULL) = (claim_until IS NULL)),
    ADD CONSTRAINT cqrs_tenant_outbox_terminal_consistency
        CHECK (published_at IS NULL OR dead_lettered_at IS NULL);

CREATE INDEX cqrs_tenant_outbox_expired_claims
    ON erpx_cqrs_write.outbox_events (claim_until, global_sequence)
    WHERE published_at IS NULL AND dead_lettered_at IS NULL;

CREATE INDEX cqrs_tenant_outbox_due_v2
    ON erpx_cqrs_write.outbox_events (available_at, global_sequence)
    WHERE published_at IS NULL AND dead_lettered_at IS NULL;

CREATE SCHEMA erpx_meta;

CREATE TABLE erpx_meta.schema_manifest (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    schema_generation SMALLINT NOT NULL CHECK (schema_generation = 2),
    manifest_version SMALLINT NOT NULL CHECK (manifest_version = 1),
    database_role TEXT NOT NULL CHECK (database_role = 'tenant_write'),
    migration_set TEXT NOT NULL CHECK (migration_set = 'tenant-write-v2'),
    installed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO erpx_meta.schema_manifest
    (singleton, schema_generation, manifest_version, database_role, migration_set)
VALUES (TRUE, 2, 1, 'tenant_write', 'tenant-write-v2');
