CREATE SCHEMA IF NOT EXISTS erpx_cqrs_write;

CREATE TABLE erpx_cqrs_write.aggregate_snapshots (
    context_name TEXT NOT NULL,
    partition_key TEXT NOT NULL,
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    state JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (context_name, partition_key, aggregate_type, aggregate_id)
);

CREATE TABLE erpx_cqrs_write.command_receipts (
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
    PRIMARY KEY (context_name, partition_key, idempotency_key),
    UNIQUE (context_name, partition_key, command_id),
    CHECK (
        (status = 'reserved' AND receipt IS NULL AND completed_at IS NULL)
        OR (status = 'completed' AND receipt IS NOT NULL AND completed_at IS NOT NULL)
    )
);

CREATE TABLE erpx_cqrs_write.audit_log (
    audit_id UUID PRIMARY KEY,
    context_name TEXT NOT NULL,
    partition_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    tenant_scope JSONB NOT NULL,
    actor JSONB NOT NULL,
    audit_context JSONB NOT NULL,
    action TEXT NOT NULL,
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    details JSONB NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    UNIQUE (context_name, partition_key, command_id, audit_id)
);

CREATE TABLE erpx_cqrs_write.outbox_events (
    global_sequence BIGSERIAL PRIMARY KEY,
    event_id UUID NOT NULL UNIQUE,
    context_name TEXT NOT NULL,
    partition_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    event_type TEXT NOT NULL,
    event_version INTEGER NOT NULL CHECK (event_version > 0),
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    tenant_scope JSONB NOT NULL,
    actor JSONB NOT NULL,
    correlation_id UUID NOT NULL,
    causation_id UUID,
    payload JSONB NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    available_at TIMESTAMPTZ NOT NULL,
    published_at TIMESTAMPTZ,
    publish_attempts INTEGER NOT NULL DEFAULT 0 CHECK (publish_attempts >= 0),
    publish_error TEXT
);

CREATE INDEX cqrs_outbox_projection_source
    ON erpx_cqrs_write.outbox_events (global_sequence)
    WHERE published_at IS NULL;

CREATE INDEX cqrs_outbox_publish_source
    ON erpx_cqrs_write.outbox_events (available_at, global_sequence)
    WHERE published_at IS NULL;

CREATE INDEX cqrs_outbox_command
    ON erpx_cqrs_write.outbox_events (context_name, partition_key, command_id);
