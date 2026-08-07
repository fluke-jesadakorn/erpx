-- Version-neutral contract. Projected context tables must include `generation`
-- so a shadow rebuild can be switched atomically without stopping writes.
CREATE SCHEMA IF NOT EXISTS erpx_cqrs_read;

CREATE TABLE IF NOT EXISTS erpx_cqrs_read.projection_generations (
    projection_name TEXT NOT NULL,
    generation UUID NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('building', 'active', 'retired')),
    source_sequence BIGINT NOT NULL DEFAULT 0 CHECK (source_sequence >= 0),
    created_at TIMESTAMPTZ NOT NULL,
    activated_at TIMESTAMPTZ,
    retired_at TIMESTAMPTZ,
    PRIMARY KEY (projection_name, generation)
);

CREATE UNIQUE INDEX IF NOT EXISTS cqrs_one_active_projection_generation
    ON erpx_cqrs_read.projection_generations (projection_name)
    WHERE status = 'active';

CREATE TABLE IF NOT EXISTS erpx_cqrs_read.projection_checkpoints (
    projection_name TEXT NOT NULL,
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    event_id UUID NOT NULL,
    global_sequence BIGINT NOT NULL CHECK (global_sequence > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS erpx_cqrs_read.projection_applied_events (
    projection_name TEXT NOT NULL,
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    event_id UUID NOT NULL,
    global_sequence BIGINT NOT NULL CHECK (global_sequence > 0),
    event_type TEXT NOT NULL,
    event_version INTEGER NOT NULL CHECK (event_version > 0),
    applied_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, event_id),
    UNIQUE (projection_name, generation, partition_key, global_sequence),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS erpx_cqrs_read.projection_receipts (
    projection_name TEXT NOT NULL,
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    event_id UUID NOT NULL,
    global_sequence BIGINT NOT NULL CHECK (global_sequence > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, command_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS erpx_cqrs_read.projection_failures (
    projection_name TEXT NOT NULL,
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    event_id UUID NOT NULL,
    global_sequence BIGINT NOT NULL CHECK (global_sequence > 0),
    attempts INTEGER NOT NULL CHECK (attempts > 0),
    next_retry_at TIMESTAMPTZ,
    dead_lettered_at TIMESTAMPTZ,
    last_error TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, event_id),
    CHECK (dead_lettered_at IS NULL OR next_retry_at IS NULL),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS cqrs_projection_retry_queue
    ON erpx_cqrs_read.projection_failures
        (projection_name, generation, next_retry_at, global_sequence)
    WHERE dead_lettered_at IS NULL;

-- Each context owns its generated read tables. Required shape:
-- CREATE TABLE <context_schema>.<read_model> (
--   generation UUID NOT NULL,
--   partition_key TEXT NOT NULL,
--   ...,
--   PRIMARY KEY (generation, partition_key, ...)
-- );
