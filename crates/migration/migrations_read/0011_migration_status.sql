-- Disposable, payload-free G13 migration status projection. Source run and
-- evidence facts remain authoritative on the control-write database.

CREATE TABLE IF NOT EXISTS erpx_control.migration_status_read_model (
    run_id UUID PRIMARY KEY,
    cohort TEXT NOT NULL,
    phase TEXT NOT NULL,
    command_owner TEXT NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    retention_until TIMESTAMPTZ,
    transition_count BIGINT NOT NULL DEFAULT 0 CHECK (transition_count >= 0),
    work_pending BIGINT NOT NULL DEFAULT 0 CHECK (work_pending >= 0),
    work_failed BIGINT NOT NULL DEFAULT 0 CHECK (work_failed >= 0),
    reconciliation_state TEXT NOT NULL,
    rollback_state TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    published_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS migration_status_read_model_phase_idx
    ON erpx_control.migration_status_read_model (phase, updated_at DESC, run_id);

CREATE TABLE IF NOT EXISTS erpx_control.migration_reconciliation_read_model (
    report_id UUID PRIMARY KEY,
    run_id UUID NOT NULL,
    context_key TEXT NOT NULL,
    state TEXT NOT NULL,
    difference_count BIGINT NOT NULL CHECK (difference_count >= 0),
    unexplained_count BIGINT NOT NULL CHECK (unexplained_count >= 0),
    evidence_hash TEXT NOT NULL CHECK (evidence_hash ~ '^[0-9a-fA-F]{64}$'),
    recorded_at TIMESTAMPTZ NOT NULL,
    published_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (run_id, context_key, evidence_hash)
);

CREATE INDEX IF NOT EXISTS migration_reconciliation_read_model_run_idx
    ON erpx_control.migration_reconciliation_read_model (run_id, context_key, recorded_at DESC);

CREATE TABLE IF NOT EXISTS erpx_control.migration_projection_checkpoints (
    projection_name TEXT PRIMARY KEY,
    source_updated_at TIMESTAMPTZ,
    source_event_id UUID,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
