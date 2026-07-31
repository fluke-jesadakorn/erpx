-- Rebuildable, payload-free G13 target read model.
-- The target write ledger and its outbox remain authoritative. Source keys and
-- payloads are represented only by SHA-256 digests in the read database.

CREATE TABLE IF NOT EXISTS erpx_tenant.g13_migration_import_read_rows (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    run_id UUID NOT NULL,
    source_kind TEXT NOT NULL CHECK (btrim(source_kind) <> ''),
    source_key_hash TEXT NOT NULL CHECK (source_key_hash ~ '^[0-9a-fA-F]{64}$'),
    target_id UUID NOT NULL,
    source_contract_hash TEXT NOT NULL CHECK (source_contract_hash ~ '^[0-9a-fA-F]{64}$'),
    source_fence TEXT NOT NULL CHECK (btrim(source_fence) <> ''),
    payload_hash TEXT NOT NULL CHECK (payload_hash ~ '^[0-9a-fA-F]{64}$'),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, run_id, source_kind, source_key_hash)
);

CREATE INDEX IF NOT EXISTS g13_migration_import_read_run_idx
    ON erpx_tenant.g13_migration_import_read_rows
        (holding_ref, run_id, source_kind, projected_at DESC);

ALTER TABLE erpx_tenant.g13_migration_import_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.g13_migration_import_read_rows FORCE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS tenant_scope_g13_migration_import_read_rows
    ON erpx_tenant.g13_migration_import_read_rows;
CREATE POLICY tenant_scope_g13_migration_import_read_rows
    ON erpx_tenant.g13_migration_import_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
