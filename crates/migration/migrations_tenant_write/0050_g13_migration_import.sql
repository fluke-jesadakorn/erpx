-- G13 target import ledger. This is tenant write state, not a read-model or
-- source database cache. Rows are immutable for a run/source fence so replay
-- cannot silently relink a legacy identity to a different ERPX identity.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.g13_migration_import_rows (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    run_id UUID NOT NULL,
    source_contract_hash TEXT NOT NULL CHECK (length(source_contract_hash) = 64),
    source_fence TEXT NOT NULL CHECK (btrim(source_fence) <> ''),
    source_kind TEXT NOT NULL CHECK (btrim(source_kind) <> ''),
    source_key TEXT NOT NULL CHECK (btrim(source_key) <> ''),
    target_id UUID NOT NULL,
    payload JSONB NOT NULL,
    imported_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, run_id, source_kind, source_key)
);

CREATE INDEX IF NOT EXISTS g13_migration_import_run_idx
    ON erpx_tenant.g13_migration_import_rows (holding_ref, run_id, source_kind, imported_at);

ALTER TABLE erpx_tenant.g13_migration_import_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.g13_migration_import_rows FORCE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS tenant_scope_g13_migration_import_rows
    ON erpx_tenant.g13_migration_import_rows;
CREATE POLICY tenant_scope_g13_migration_import_rows
    ON erpx_tenant.g13_migration_import_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

-- A rollback rehearsal is kept in a separate tenant write database. It is
-- never used as a command source and is safe to discard after retention.
CREATE TABLE IF NOT EXISTS erpx_tenant.g13_migration_rollback_rows (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    run_id UUID NOT NULL,
    source_contract_hash TEXT NOT NULL CHECK (length(source_contract_hash) = 64),
    source_fence TEXT NOT NULL CHECK (btrim(source_fence) <> ''),
    source_kind TEXT NOT NULL CHECK (btrim(source_kind) <> ''),
    source_key TEXT NOT NULL CHECK (btrim(source_key) <> ''),
    target_id UUID NOT NULL,
    payload JSONB NOT NULL,
    captured_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, run_id, source_kind, source_key)
);

CREATE INDEX IF NOT EXISTS g13_migration_rollback_run_idx
    ON erpx_tenant.g13_migration_rollback_rows (holding_ref, run_id, source_kind, captured_at);

ALTER TABLE erpx_tenant.g13_migration_rollback_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.g13_migration_rollback_rows FORCE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS tenant_scope_g13_migration_rollback_rows
    ON erpx_tenant.g13_migration_rollback_rows;
CREATE POLICY tenant_scope_g13_migration_rollback_rows
    ON erpx_tenant.g13_migration_rollback_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
