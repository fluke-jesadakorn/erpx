-- Rebuildable, secret-free read projection for backup object inventory drift.
CREATE TABLE IF NOT EXISTS erpx_control.tenant_backup_inventory_reports (
    id UUID PRIMARY KEY,
    retention_action_id UUID NOT NULL,
    tenant_id TEXT NOT NULL,
    holding_ref TEXT NOT NULL,
    expected_objects INTEGER NOT NULL CHECK (expected_objects >= 0),
    observed_objects INTEGER NOT NULL CHECK (observed_objects >= 0),
    missing_objects INTEGER NOT NULL CHECK (missing_objects >= 0),
    unexpected_objects INTEGER NOT NULL CHECK (unexpected_objects >= 0),
    state TEXT NOT NULL CHECK (state IN ('matched', 'drifted', 'unavailable')),
    error_present BOOLEAN NOT NULL,
    checked_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS tenant_backup_inventory_reports_read_history_idx
    ON erpx_control.tenant_backup_inventory_reports
        (tenant_id, holding_ref, checked_at DESC, id DESC);
