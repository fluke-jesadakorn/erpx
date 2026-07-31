-- Durable, append-only G13 migration execution evidence. These tables are
-- coordination and evidence state only; they never switch production command
-- ownership or mutate Legacy business data.
-- Production command ownership remains unchanged until explicit approval.

CREATE TABLE IF NOT EXISTS erpx_control.migration_runs (
    run_id UUID PRIMARY KEY,
    cohort TEXT NOT NULL CHECK (length(trim(cohort)) BETWEEN 1 AND 128),
    source_contract_hash TEXT NOT NULL CHECK (source_contract_hash ~ '^[0-9a-fA-F]{64}$'),
    id_namespace TEXT NOT NULL CHECK (length(trim(id_namespace)) BETWEEN 1 AND 128),
    phase TEXT NOT NULL CHECK (phase IN (
        'source_frozen', 'ids_mapped', 'write_backfilled', 'read_projected',
        'shadow_replaying', 'reconciled', 'read_only_pilot',
        'workflow_accepted', 'rollback_rehearsed', 'mutation_frozen',
        'final_delta_imported', 'erpx_command_owner', 'legacy_read_only',
        'retention_complete', 'legacy_retired'
    )),
    command_owner TEXT NOT NULL CHECK (command_owner IN ('legacy', 'erpx')),
    retention_until TIMESTAMPTZ,
    version BIGINT NOT NULL CHECK (version > 0),
    transitioned_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (cohort),
    CHECK (
        (phase IN ('erpx_command_owner', 'legacy_read_only', 'retention_complete', 'legacy_retired'))
        = (command_owner = 'erpx')
    )
);

CREATE INDEX IF NOT EXISTS migration_runs_phase_idx
    ON erpx_control.migration_runs (phase, updated_at DESC, run_id);

CREATE TABLE IF NOT EXISTS erpx_control.migration_transition_events (
    event_id UUID PRIMARY KEY,
    run_id UUID NOT NULL REFERENCES erpx_control.migration_runs(run_id) ON DELETE RESTRICT,
    from_phase TEXT NOT NULL,
    to_phase TEXT NOT NULL,
    command_owner TEXT NOT NULL CHECK (command_owner IN ('legacy', 'erpx')),
    actor_ref TEXT NOT NULL CHECK (length(trim(actor_ref)) BETWEEN 1 AND 200),
    correlation_id UUID NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (length(trim(idempotency_key)) BETWEEN 1 AND 200),
    request_hash TEXT NOT NULL CHECK (request_hash ~ '^[0-9a-fA-F]{64}$'),
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (run_id, idempotency_key)
);

CREATE INDEX IF NOT EXISTS migration_transition_events_run_idx
    ON erpx_control.migration_transition_events (run_id, occurred_at, event_id);

CREATE TABLE IF NOT EXISTS erpx_control.migration_idempotency (
    operation TEXT NOT NULL CHECK (length(trim(operation)) BETWEEN 1 AND 100),
    idempotency_key TEXT NOT NULL CHECK (length(trim(idempotency_key)) BETWEEN 1 AND 200),
    request_hash TEXT NOT NULL CHECK (request_hash ~ '^[0-9a-fA-F]{64}$'),
    run_id UUID REFERENCES erpx_control.migration_runs(run_id) ON DELETE RESTRICT,
    response JSONB,
    state TEXT NOT NULL CHECK (state IN ('reserved', 'completed', 'failed')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    PRIMARY KEY (operation, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_control.migration_work_items (
    work_id UUID PRIMARY KEY,
    run_id UUID NOT NULL REFERENCES erpx_control.migration_runs(run_id) ON DELETE RESTRICT,
    work_kind TEXT NOT NULL CHECK (work_kind IN (
        'backfill', 'project', 'shadow_replay', 'final_delta', 'reconcile', 'rollback'
    )),
    scope_key TEXT NOT NULL CHECK (length(trim(scope_key)) BETWEEN 1 AND 200),
    source_fence TEXT NOT NULL CHECK (length(trim(source_fence)) BETWEEN 1 AND 200),
    request_hash TEXT NOT NULL CHECK (request_hash ~ '^[0-9a-fA-F]{64}$'),
    state TEXT NOT NULL CHECK (state IN (
        'pending', 'claimed', 'retryable', 'succeeded', 'failed', 'dead_letter'
    )),
    available_at TIMESTAMPTZ NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 25),
    max_attempts INTEGER NOT NULL CHECK (max_attempts BETWEEN 1 AND 25),
    worker_id UUID,
    claim_id UUID,
    lease_until TIMESTAMPTZ,
    last_error TEXT CHECK (last_error IS NULL OR octet_length(last_error) <= 1000),
    result JSONB NOT NULL DEFAULT '{}'::jsonb,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((state = 'claimed') = (worker_id IS NOT NULL AND claim_id IS NOT NULL AND lease_until IS NOT NULL)),
    UNIQUE (run_id, work_kind, scope_key, request_hash)
);

CREATE INDEX IF NOT EXISTS migration_work_items_due_idx
    ON erpx_control.migration_work_items (available_at, work_id)
    WHERE state IN ('pending', 'retryable');

CREATE INDEX IF NOT EXISTS migration_work_items_lease_idx
    ON erpx_control.migration_work_items (lease_until, work_id)
    WHERE state = 'claimed';

CREATE TABLE IF NOT EXISTS erpx_control.migration_checkpoints (
    run_id UUID NOT NULL REFERENCES erpx_control.migration_runs(run_id) ON DELETE RESTRICT,
    work_kind TEXT NOT NULL,
    scope_key TEXT NOT NULL,
    source_contract_hash TEXT CHECK (source_contract_hash IS NULL OR source_contract_hash ~ '^[0-9a-fA-F]{64}$'),
    source_cursor TEXT,
    source_version BIGINT NOT NULL DEFAULT 0 CHECK (source_version >= 0),
    rows_seen BIGINT NOT NULL DEFAULT 0 CHECK (rows_seen >= 0),
    rows_applied BIGINT NOT NULL DEFAULT 0 CHECK (rows_applied >= 0),
    rows_replayed BIGINT NOT NULL DEFAULT 0 CHECK (rows_replayed >= 0),
    source_digest TEXT CHECK (source_digest IS NULL OR source_digest ~ '^[0-9a-fA-F]{64}$'),
    state TEXT NOT NULL CHECK (state IN ('pending', 'running', 'succeeded', 'failed', 'rolled_back')),
    checkpoint_version BIGINT NOT NULL DEFAULT 1 CHECK (checkpoint_version > 0),
    idempotency_key TEXT CHECK (idempotency_key IS NULL OR length(trim(idempotency_key)) BETWEEN 1 AND 200),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (run_id, work_kind, scope_key)
);

CREATE TABLE IF NOT EXISTS erpx_control.migration_reconciliation_reports (
    report_id UUID PRIMARY KEY,
    run_id UUID NOT NULL REFERENCES erpx_control.migration_runs(run_id) ON DELETE RESTRICT,
    context_key TEXT NOT NULL CHECK (length(trim(context_key)) BETWEEN 1 AND 200),
    source_counts JSONB NOT NULL,
    target_counts JSONB NOT NULL,
    source_totals JSONB NOT NULL,
    target_totals JSONB NOT NULL,
    differences JSONB NOT NULL,
    unexplained JSONB NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('matched', 'different', 'unexplained', 'unavailable')),
    evidence_hash TEXT NOT NULL CHECK (evidence_hash ~ '^[0-9a-fA-F]{64}$'),
    actor_ref TEXT NOT NULL CHECK (length(trim(actor_ref)) BETWEEN 1 AND 200),
    correlation_id UUID NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (run_id, context_key, evidence_hash)
);

CREATE INDEX IF NOT EXISTS migration_reconciliation_reports_run_idx
    ON erpx_control.migration_reconciliation_reports (run_id, context_key, recorded_at DESC);

CREATE TABLE IF NOT EXISTS erpx_control.migration_rollback_rehearsals (
    rehearsal_id UUID PRIMARY KEY,
    run_id UUID NOT NULL REFERENCES erpx_control.migration_runs(run_id) ON DELETE RESTRICT,
    target_snapshot_ref TEXT NOT NULL CHECK (length(trim(target_snapshot_ref)) BETWEEN 1 AND 200),
    pre_rollback_digest TEXT NOT NULL CHECK (pre_rollback_digest ~ '^[0-9a-fA-F]{64}$'),
    restored_digest TEXT CHECK (restored_digest IS NULL OR restored_digest ~ '^[0-9a-fA-F]{64}$'),
    state TEXT NOT NULL CHECK (state IN ('started', 'succeeded', 'failed')),
    source_preserved BOOLEAN NOT NULL DEFAULT FALSE,
    read_model_rebuilt BOOLEAN NOT NULL DEFAULT FALSE,
    error_present BOOLEAN NOT NULL DEFAULT FALSE,
    actor_ref TEXT NOT NULL CHECK (length(trim(actor_ref)) BETWEEN 1 AND 200),
    correlation_id UUID NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (run_id, target_snapshot_ref, pre_rollback_digest)
);

CREATE INDEX IF NOT EXISTS migration_rollback_rehearsals_run_idx
    ON erpx_control.migration_rollback_rehearsals (run_id, recorded_at DESC);
