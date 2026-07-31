-- Secret-free evidence for comparing retention-selected backup references with
-- the tenant-bound object-store inventory. The write-side retention claim is
-- authoritative; this report is appendable/replayable worker evidence.
CREATE TABLE IF NOT EXISTS erpx_control.tenant_backup_inventory_reports (
    id UUID PRIMARY KEY,
    retention_action_id UUID NOT NULL
        REFERENCES erpx_control.tenant_backup_retention_actions(id) ON DELETE RESTRICT,
    tenant_id TEXT NOT NULL,
    holding_ref TEXT NOT NULL CHECK (length(trim(holding_ref)) BETWEEN 1 AND 200),
    expected_objects INTEGER NOT NULL CHECK (expected_objects >= 0),
    observed_objects INTEGER NOT NULL CHECK (observed_objects >= 0),
    missing_objects INTEGER NOT NULL CHECK (missing_objects >= 0),
    unexpected_objects INTEGER NOT NULL CHECK (unexpected_objects >= 0),
    state TEXT NOT NULL CHECK (state IN ('matched', 'drifted', 'unavailable')),
    error_present BOOLEAN NOT NULL DEFAULT FALSE,
    checked_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (retention_action_id),
    CHECK (missing_objects <= expected_objects)
);

CREATE INDEX IF NOT EXISTS tenant_backup_inventory_reports_history_idx
    ON erpx_control.tenant_backup_inventory_reports
        (tenant_id, holding_ref, checked_at DESC, id DESC);
