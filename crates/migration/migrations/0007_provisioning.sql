CREATE TABLE IF NOT EXISTS erpx_control.tenant_provisioning (
    tenant_id TEXT PRIMARY KEY REFERENCES erpx_control.tenant_registry(tenant_id) ON DELETE RESTRICT,
    state TEXT NOT NULL CHECK (state IN ('provisioning', 'active', 'suspended', 'recovery', 'failed')),
    write_database_key TEXT NOT NULL,
    read_database_key TEXT NOT NULL,
    storage_namespace TEXT NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    last_error TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_control.tenant_database_health (
    tenant_id TEXT NOT NULL REFERENCES erpx_control.tenant_registry(tenant_id) ON DELETE RESTRICT,
    side TEXT NOT NULL CHECK (side IN ('write', 'read')),
    database_key TEXT NOT NULL,
    schema_version TEXT NOT NULL,
    reachable BOOLEAN NOT NULL,
    checked_at TIMESTAMPTZ NOT NULL,
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    PRIMARY KEY (tenant_id, side)
);

CREATE TABLE IF NOT EXISTS erpx_control.tenant_storage_health (
    tenant_id TEXT PRIMARY KEY REFERENCES erpx_control.tenant_registry(tenant_id) ON DELETE RESTRICT,
    storage_namespace TEXT NOT NULL,
    reachable BOOLEAN NOT NULL,
    checked_at TIMESTAMPTZ NOT NULL,
    details JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE TABLE IF NOT EXISTS erpx_control.tenant_backup_runs (
    id UUID PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES erpx_control.tenant_registry(tenant_id) ON DELETE RESTRICT,
    kind TEXT NOT NULL CHECK (kind IN ('full', 'pitr', 'restore_test')),
    status TEXT NOT NULL CHECK (status IN ('started', 'completed', 'failed')),
    backup_ref TEXT NOT NULL,
    restore_target_ref TEXT,
    started_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    verified BOOLEAN NOT NULL DEFAULT FALSE,
    error TEXT,
    CHECK (btrim(backup_ref) <> ''),
    CHECK ((kind <> 'restore_test') OR restore_target_ref IS NOT NULL)
);

CREATE INDEX IF NOT EXISTS tenant_backup_runs_history_idx
    ON erpx_control.tenant_backup_runs (tenant_id, started_at DESC);

CREATE TABLE IF NOT EXISTS erpx_control.tenant_projection_switches (
    tenant_id TEXT PRIMARY KEY REFERENCES erpx_control.tenant_registry(tenant_id) ON DELETE RESTRICT,
    current_read_database_key TEXT NOT NULL,
    candidate_read_database_key TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('rebuilding', 'ready', 'activated', 'failed')),
    source_version BIGINT NOT NULL CHECK (source_version >= 0),
    version BIGINT NOT NULL CHECK (version > 0),
    last_error TEXT,
    activated_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (current_read_database_key <> candidate_read_database_key)
);
