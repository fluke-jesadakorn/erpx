-- Disposable Legal relationship graph projection rebuilt from matter snapshots.

CREATE TABLE erpx_legal_read.matter_projection (
    projection_name TEXT NOT NULL DEFAULT 'legal.matters'
        CHECK (projection_name = 'legal.matters'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    matter_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    reference TEXT NOT NULL,
    title TEXT NOT NULL,
    matter_status TEXT NOT NULL CHECK (matter_status IN ('open', 'closed')),
    links JSONB NOT NULL CHECK (jsonb_typeof(links) = 'array'),
    legal_hold JSONB NOT NULL,
    created_by TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, matter_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, scope_key, reference)
);

CREATE INDEX legal_read_matter_scope_status
    ON erpx_legal_read.matter_projection
        (generation, scope_key, matter_status, aggregate_updated_at DESC, matter_id DESC);
CREATE INDEX legal_read_matter_links_gin
    ON erpx_legal_read.matter_projection USING GIN (links jsonb_path_ops);
