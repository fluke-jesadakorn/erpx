-- Rebuildable, payload-bounded platform-control read model. Authoritative
-- lifecycle, backup, connector, and replacement-read state remains on the
-- control write database; this table is disposable and projector-owned.
CREATE TABLE IF NOT EXISTS erpx_control.platform_completion_read_model (
    projection_name TEXT NOT NULL,
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    tenant_id TEXT,
    holding_ref TEXT,
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, aggregate_type, aggregate_id)
);

CREATE INDEX IF NOT EXISTS platform_completion_read_model_scope_idx
    ON erpx_control.platform_completion_read_model
        (projection_name, tenant_id, holding_ref, updated_at DESC);
