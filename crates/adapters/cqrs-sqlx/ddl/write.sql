-- Version-neutral contract. The migration owner must allocate a migration
-- number and may select a deployment-specific schema prefix.
CREATE SCHEMA IF NOT EXISTS erpx_cqrs_write;

CREATE OR REPLACE FUNCTION erpx_cqrs_write.current_holding_id()
RETURNS UUID LANGUAGE SQL STABLE AS $$
    SELECT nullif(current_setting('app.holding_id', true), '')::uuid
$$;

CREATE TABLE IF NOT EXISTS erpx_cqrs_write.command_receipts (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    context_name TEXT NOT NULL,
    partition_key TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash) = 32),
    correlation_id UUID NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('reserved', 'completed')),
    receipt JSONB,
    created_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    PRIMARY KEY (holding_id, context_name, partition_key, idempotency_key),
    UNIQUE (holding_id, context_name, partition_key, command_id),
    CHECK (
        (status = 'reserved' AND receipt IS NULL AND completed_at IS NULL)
        OR (status = 'completed' AND receipt IS NOT NULL AND completed_at IS NOT NULL)
    )
);

CREATE TABLE IF NOT EXISTS erpx_cqrs_write.audit_log (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    audit_id UUID NOT NULL,
    context_name TEXT NOT NULL,
    partition_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    active_scope JSONB NOT NULL,
    actor JSONB NOT NULL,
    audit_context JSONB NOT NULL,
    action TEXT NOT NULL,
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    details JSONB NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, audit_id),
    UNIQUE (holding_id, context_name, partition_key, command_id, audit_id)
);

CREATE TABLE IF NOT EXISTS erpx_cqrs_write.outbox_events (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    global_sequence BIGSERIAL NOT NULL,
    event_id UUID NOT NULL,
    context_name TEXT NOT NULL,
    partition_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    event_type TEXT NOT NULL,
    event_version INTEGER NOT NULL CHECK (event_version > 0),
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    active_scope JSONB NOT NULL,
    actor JSONB NOT NULL,
    correlation_id UUID NOT NULL,
    causation_id UUID,
    payload JSONB NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    available_at TIMESTAMPTZ NOT NULL,
    published_at TIMESTAMPTZ,
    publish_attempts INTEGER NOT NULL DEFAULT 0 CHECK (publish_attempts >= 0),
    publish_error TEXT,
    claim_token UUID,
    claim_until TIMESTAMPTZ,
    dead_lettered_at TIMESTAMPTZ,
    CHECK ((claim_token IS NULL) = (claim_until IS NULL)),
    CHECK (published_at IS NULL OR dead_lettered_at IS NULL),
    PRIMARY KEY (holding_id, global_sequence),
    UNIQUE (holding_id, event_id)
);

-- A projection cursor needs commit order, while BIGSERIAL alone supplies only
-- allocation order. Acquire the holding-local transaction lock before nextval;
-- it remains held until commit and applies to direct INSERT producers as well
-- as the shared command adapter. The trigger owns allocation and rejects an
-- explicitly supplied sequence; ALTER keeps this contract effective when the
-- version-neutral DDL is reconciled against an existing table.
CREATE OR REPLACE FUNCTION erpx_cqrs_write.lock_outbox_commit_order()
RETURNS VOID
LANGUAGE plpgsql
VOLATILE
AS $$
DECLARE
    bound_holding UUID := erpx_cqrs_write.current_holding_id();
BEGIN
    IF bound_holding IS NULL THEN
        RAISE EXCEPTION 'holding authority is required for outbox sequence allocation'
            USING ERRCODE = '42501';
    END IF;
    PERFORM pg_catalog.pg_advisory_xact_lock(
        1163022424,
        pg_catalog.hashtext(bound_holding::TEXT)
    );
END;
$$;

CREATE OR REPLACE FUNCTION erpx_cqrs_write.assign_outbox_global_sequence()
RETURNS TRIGGER
LANGUAGE plpgsql
VOLATILE
AS $$
BEGIN
    IF NEW.global_sequence IS NOT NULL THEN
        RAISE EXCEPTION 'global_sequence is database assigned'
            USING ERRCODE = '22023';
    END IF;
    PERFORM erpx_cqrs_write.lock_outbox_commit_order();
    NEW.global_sequence := pg_catalog.nextval(
        'erpx_cqrs_write.outbox_events_global_sequence_seq'::REGCLASS
    );
    RETURN NEW;
END;
$$;

ALTER TABLE erpx_cqrs_write.outbox_events
    ALTER COLUMN global_sequence
    DROP DEFAULT;

DROP TRIGGER IF EXISTS cqrs_outbox_assign_global_sequence
    ON erpx_cqrs_write.outbox_events;
CREATE TRIGGER cqrs_outbox_assign_global_sequence
BEFORE INSERT ON erpx_cqrs_write.outbox_events
FOR EACH ROW
EXECUTE FUNCTION erpx_cqrs_write.assign_outbox_global_sequence();

CREATE INDEX IF NOT EXISTS cqrs_outbox_projection_source
    ON erpx_cqrs_write.outbox_events (holding_id, global_sequence)
    WHERE published_at IS NULL;

CREATE INDEX IF NOT EXISTS cqrs_outbox_publish_source
    ON erpx_cqrs_write.outbox_events (holding_id, available_at, global_sequence)
    WHERE published_at IS NULL AND dead_lettered_at IS NULL;

CREATE INDEX IF NOT EXISTS cqrs_outbox_expired_claims
    ON erpx_cqrs_write.outbox_events (holding_id, claim_until, global_sequence)
    WHERE published_at IS NULL AND dead_lettered_at IS NULL;

CREATE INDEX IF NOT EXISTS cqrs_outbox_command
    ON erpx_cqrs_write.outbox_events (holding_id, context_name, partition_key, command_id);

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'command_receipts','audit_log','outbox_events'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_cqrs_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_cqrs_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY cqrs_write_holding_isolation ON erpx_cqrs_write.%I '
            'USING (holding_id = erpx_cqrs_write.current_holding_id()) '
            'WITH CHECK (holding_id = erpx_cqrs_write.current_holding_id())', table_name
        );
    END LOOP;
END;
$$;
