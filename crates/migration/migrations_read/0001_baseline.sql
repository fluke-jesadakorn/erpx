
CREATE SCHEMA IF NOT EXISTS erpx_control;

-- Rebuildable projection storage only. Aggregate snapshots, outbox events,
-- role assignments, identity, and command receipts exist on the write side.
CREATE TABLE IF NOT EXISTS erpx_control.customers (
    id UUID PRIMARY KEY,
    account_code TEXT NOT NULL UNIQUE,
    trading_name TEXT NOT NULL,
    legal_name TEXT,
    country TEXT NOT NULL,
    status TEXT NOT NULL,
    billing_state TEXT NOT NULL,
    search_document TSVECTOR GENERATED ALWAYS AS (
        to_tsvector('simple', coalesce(account_code, '') || ' ' || coalesce(trading_name, '') || ' ' || coalesce(legal_name, ''))
    ) STORED,
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS customers_search_document_idx
    ON erpx_control.customers USING GIN (search_document);


CREATE TABLE IF NOT EXISTS erpx_control.customer_access_audit_summaries (
    customer_id UUID PRIMARY KEY REFERENCES erpx_control.customers(id) ON DELETE CASCADE,
    read_count BIGINT NOT NULL DEFAULT 0 CHECK (read_count >= 0),
    last_access_at TIMESTAMPTZ,
    customer_visible BOOLEAN NOT NULL DEFAULT TRUE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_control.customer_notes (
    id UUID PRIMARY KEY,
    customer_id UUID NOT NULL REFERENCES erpx_control.customers(id) ON DELETE CASCADE,
    author_ref TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    body TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS customer_notes_customer_idx
    ON erpx_control.customer_notes (customer_id, created_at DESC, id);


CREATE SCHEMA IF NOT EXISTS erpx_control;

-- Rebuildable, safe incident summaries. Internal timeline, broadcasts,
-- mitigation, root cause, and corrective-action content stays on the write
-- side until a separate disclosure policy exists.
CREATE TABLE IF NOT EXISTS erpx_control.incidents (
    id UUID PRIMARY KEY,
    status TEXT NOT NULL CHECK (
        status IN ('investigating', 'identified', 'monitoring', 'resolved', 'postmortem_complete')
    ),
    commander_ref TEXT NOT NULL CHECK (btrim(commander_ref) <> ''),
    leads JSONB NOT NULL CHECK (jsonb_typeof(leads) = 'array'),
    affected_services JSONB NOT NULL CHECK (jsonb_typeof(affected_services) = 'array'),
    affected_customer_accounts JSONB NOT NULL CHECK (jsonb_typeof(affected_customer_accounts) = 'array'),
    version BIGINT NOT NULL CHECK (version > 0),
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS incidents_status_updated_idx
    ON erpx_control.incidents (status, updated_at DESC, id DESC);


CREATE SCHEMA IF NOT EXISTS erpx_control;

-- Rebuildable scheduler status metadata. Detailed source and execution
-- material remains on the control-write side.
CREATE TABLE IF NOT EXISTS erpx_control.holding_projection_status (
    holding_id TEXT NOT NULL CHECK (length(btrim(holding_id)) BETWEEN 1 AND 200),
    data_profile_key TEXT NOT NULL CHECK (length(btrim(data_profile_key)) BETWEEN 1 AND 200),
    projection_name TEXT NOT NULL CHECK (
        length(trim(projection_name)) BETWEEN 1 AND 120
        AND projection_name ~ '^[A-Za-z0-9_.-]+$'
    ),
    job_key TEXT NOT NULL CHECK (length(btrim(job_key)) BETWEEN 1 AND 200),
    job_id UUID PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('incremental', 'rebuild')),
    state TEXT NOT NULL CHECK (
        state IN (
            'pending', 'claimed', 'retryable', 'succeeded', 'dead_letter',
            'rebuild_pending', 'rebuild_succeeded'
        )
    ),
    available_at TIMESTAMPTZ NOT NULL,
    attempts INTEGER NOT NULL CHECK (attempts BETWEEN 0 AND 25),
    max_attempts INTEGER NOT NULL CHECK (max_attempts BETWEEN 1 AND 25),
    lease_until TIMESTAMPTZ,
    last_error TEXT CHECK (last_error IS NULL OR octet_length(last_error) <= 1000),
    dead_lettered_at TIMESTAMPTZ,
    rebuild_id UUID,
    version BIGINT NOT NULL CHECK (version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    published_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT holding_projection_status_kind_state_consistent CHECK (
        (kind = 'rebuild') OR state NOT IN ('rebuild_pending', 'rebuild_succeeded')
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS holding_projection_status_order_idx
    ON erpx_control.holding_projection_status
       (holding_id, data_profile_key, projection_name, job_key, job_id);

CREATE INDEX IF NOT EXISTS holding_projection_status_updated_idx
    ON erpx_control.holding_projection_status (updated_at DESC, job_id DESC);


ALTER TABLE erpx_control.holding_projection_status
    ADD COLUMN IF NOT EXISTS published_at TIMESTAMPTZ NOT NULL DEFAULT now();

CREATE INDEX IF NOT EXISTS holding_projection_status_published_idx
    ON erpx_control.holding_projection_status (published_at DESC, job_id DESC);


CREATE SCHEMA IF NOT EXISTS erpx_control;

-- A single durable source cursor for the metadata-only scheduler status
-- projection. The source tuple is strictly ordered by mutable source update
-- time and job identity, so a retry can replay a page without skipping an
-- update to an older job.
CREATE TABLE IF NOT EXISTS erpx_control.holding_projection_status_checkpoints (
    projection_name TEXT PRIMARY KEY CHECK (
        length(trim(projection_name)) BETWEEN 1 AND 120
        AND projection_name ~ '^[A-Za-z0-9_.-]+$'
    ),
    source_updated_at TIMESTAMPTZ,
    source_job_id UUID,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT holding_projection_status_checkpoint_pair CHECK (
        (source_updated_at IS NULL) = (source_job_id IS NULL)
    )
);


-- Rebuildable, secret-free control read projection for holding backup history.
-- The write-side job table remains authoritative; workers refresh this table.
CREATE TABLE IF NOT EXISTS erpx_control.holding_backup_jobs (
    id UUID PRIMARY KEY,
    holding_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('full', 'restore_test')),
    state TEXT NOT NULL CHECK (
        state IN ('pending', 'claimed', 'retryable', 'completed', 'failed', 'dead_letter')
    ),
    available_at TIMESTAMPTZ NOT NULL,
    attempts INTEGER NOT NULL CHECK (attempts >= 0),
    max_attempts INTEGER NOT NULL CHECK (max_attempts BETWEEN 1 AND 5),
    verified BOOLEAN NOT NULL,
    error_present BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS holding_backup_jobs_read_history_idx
    ON erpx_control.holding_backup_jobs (holding_id, created_at DESC, id DESC);


-- Rebuildable, secret-free read projection for backup retention actions.
-- No backup, restore, storage, credential, or error text is projected.
CREATE TABLE IF NOT EXISTS erpx_control.holding_backup_retention_actions (
    id UUID PRIMARY KEY,
    holding_id TEXT NOT NULL,
    data_profile_key TEXT NOT NULL,
    cutoff_at TIMESTAMPTZ NOT NULL,
    max_rows INTEGER NOT NULL CHECK (max_rows BETWEEN 1 AND 1000),
    dry_run BOOLEAN NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('pending', 'claimed', 'completed', 'failed')),
    attempts INTEGER NOT NULL CHECK (attempts BETWEEN 0 AND 3),
    max_attempts INTEGER NOT NULL CHECK (max_attempts BETWEEN 1 AND 3),
    jobs_selected INTEGER NOT NULL CHECK (jobs_selected >= 0),
    jobs_deleted INTEGER NOT NULL CHECK (jobs_deleted >= 0),
    runs_deleted INTEGER NOT NULL CHECK (runs_deleted >= 0),
    object_refs_released INTEGER NOT NULL CHECK (object_refs_released >= 0),
    remaining_eligible INTEGER NOT NULL CHECK (remaining_eligible >= 0),
    error_present BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS holding_backup_retention_actions_read_history_idx
    ON erpx_control.holding_backup_retention_actions
        (holding_id, data_profile_key, created_at DESC, id DESC);


-- Rebuildable, secret-free read projection for backup object inventory drift.
CREATE TABLE IF NOT EXISTS erpx_control.holding_backup_inventory_reports (
    id UUID PRIMARY KEY,
    retention_action_id UUID NOT NULL,
    holding_id TEXT NOT NULL,
    data_profile_key TEXT NOT NULL,
    expected_objects INTEGER NOT NULL CHECK (expected_objects >= 0),
    observed_objects INTEGER NOT NULL CHECK (observed_objects >= 0),
    missing_objects INTEGER NOT NULL CHECK (missing_objects >= 0),
    unexpected_objects INTEGER NOT NULL CHECK (unexpected_objects >= 0),
    state TEXT NOT NULL CHECK (state IN ('matched', 'drifted', 'unavailable')),
    error_present BOOLEAN NOT NULL,
    checked_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS holding_backup_inventory_reports_read_history_idx
    ON erpx_control.holding_backup_inventory_reports
        (holding_id, data_profile_key, checked_at DESC, id DESC);


-- Rebuildable, payload-bounded platform-control read model. Authoritative
-- lifecycle, backup, connector, and replacement-read state remains on the
-- control write database; this table is disposable and projector-owned.
CREATE TABLE IF NOT EXISTS erpx_control.platform_completion_read_model (
    projection_name TEXT NOT NULL,
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    holding_id TEXT,
    data_profile_key TEXT,
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, aggregate_type, aggregate_id)
);

CREATE INDEX IF NOT EXISTS platform_completion_read_model_scope_idx
    ON erpx_control.platform_completion_read_model
        (projection_name, holding_id, data_profile_key, updated_at DESC);


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


-- Rebuildable, payload-free source snapshot status projection.

ALTER TABLE erpx_control.migration_status_read_model
    ADD COLUMN IF NOT EXISTS source_snapshot_hash TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS source_missing_table_count BIGINT NOT NULL DEFAULT 0
        CHECK (source_missing_table_count >= 0),
    ADD COLUMN IF NOT EXISTS source_entity_kind_count BIGINT NOT NULL DEFAULT 0
        CHECK (source_entity_kind_count >= 0),
    ADD COLUMN IF NOT EXISTS source_total_rows BIGINT NOT NULL DEFAULT 0
        CHECK (source_total_rows >= 0),
    ADD COLUMN IF NOT EXISTS source_captured_at TIMESTAMPTZ;


-- Rebuildable read projection vocabulary for durable physical PITR jobs.
ALTER TABLE erpx_control.holding_backup_jobs
    DROP CONSTRAINT IF EXISTS holding_backup_jobs_kind_check;

ALTER TABLE erpx_control.holding_backup_jobs
    ADD CONSTRAINT holding_backup_jobs_kind_check
        CHECK (kind IN ('full', 'pitr', 'restore_test'));


-- Rebuildable control-read status for Finance bank-provider import jobs.
-- It contains no request, authorization, credential, or provider payload.
CREATE SCHEMA IF NOT EXISTS erpx_control;

CREATE TABLE IF NOT EXISTS erpx_control.finance_bank_provider_import_job_status (
    job_id UUID PRIMARY KEY,
    holding_id TEXT NOT NULL,
    data_profile_key TEXT NOT NULL,
    import_id UUID NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('pending', 'claimed', 'retryable', 'completed', 'dead_letter')),
    source_fence BIGINT NOT NULL CHECK (source_fence > 0),
    attempts SMALLINT NOT NULL CHECK (attempts BETWEEN 0 AND 8),
    max_attempts SMALLINT NOT NULL CHECK (max_attempts BETWEEN 1 AND 8),
    available_at TIMESTAMPTZ NOT NULL,
    lease_until TIMESTAMPTZ,
    last_failure_class TEXT,
    completed_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS finance_bank_provider_import_status_scope_idx
    ON erpx_control.finance_bank_provider_import_job_status (holding_id, data_profile_key, updated_at DESC, job_id);


-- Rebuildable control-plane CQRS projections. Platform catalog plans live in
-- control read storage so one authoritative catalog is not duplicated across
-- holding databases.

CREATE SCHEMA IF NOT EXISTS erpx_cqrs_read;

CREATE TABLE erpx_cqrs_read.projection_generations (
    projection_name TEXT NOT NULL,
    generation UUID NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('building', 'active', 'retired')),
    source_sequence BIGINT NOT NULL DEFAULT 0 CHECK (source_sequence >= 0),
    created_at TIMESTAMPTZ NOT NULL,
    activated_at TIMESTAMPTZ,
    retired_at TIMESTAMPTZ,
    PRIMARY KEY (projection_name, generation)
);

CREATE UNIQUE INDEX cqrs_control_one_active_projection_generation
    ON erpx_cqrs_read.projection_generations (projection_name)
    WHERE status = 'active';

CREATE TABLE erpx_cqrs_read.projection_checkpoints (
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

CREATE TABLE erpx_cqrs_read.projection_applied_events (
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

CREATE TABLE erpx_cqrs_read.projection_receipts (
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

CREATE TABLE erpx_cqrs_read.projection_failures (
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

CREATE INDEX cqrs_control_projection_retry_queue
    ON erpx_cqrs_read.projection_failures
        (projection_name, generation, next_retry_at, global_sequence)
    WHERE dead_lettered_at IS NULL;

CREATE SCHEMA IF NOT EXISTS erpx_commercial_read;

CREATE TABLE erpx_commercial_read.catalog_plan_projection (
    projection_name TEXT NOT NULL DEFAULT 'commercial.catalog_plans'
        CHECK (projection_name = 'commercial.catalog_plans'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    catalog_plan_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    plan_key TEXT NOT NULL,
    display_name TEXT NOT NULL,
    amount_minor BIGINT NOT NULL CHECK (amount_minor > 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    plan_status TEXT NOT NULL CHECK (plan_status IN ('draft', 'published')),
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, catalog_plan_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, plan_key)
);

CREATE INDEX commercial_control_catalog_status
    ON erpx_commercial_read.catalog_plan_projection
        (generation, plan_status, aggregate_updated_at DESC, catalog_plan_id DESC);



-- Disposable Wave 5 control-plane projections. Every table is generation
-- scoped so rebuilds can switch atomically without stopping writes.

CREATE SCHEMA IF NOT EXISTS erpx_platform_administration_read;

CREATE TABLE erpx_platform_administration_read.holding_projection (
    projection_name TEXT NOT NULL DEFAULT 'platform_administration.holdings'
        CHECK (projection_name = 'platform_administration.holdings'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    holding_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_code TEXT NOT NULL,
    holding_name TEXT NOT NULL,
    environment_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    canonical_holding_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL CHECK (lifecycle_state IN (
        'pending_subscription', 'provisioning', 'provisioning_failed',
        'active', 'suspended', 'recovery', 'retired'
    )),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, holding_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, holding_code),
    UNIQUE (generation, customer_id),
    UNIQUE (generation, canonical_holding_id),
    UNIQUE (generation, data_profile_key)
);

CREATE INDEX platform_admin_read_holding_state
    ON erpx_platform_administration_read.holding_projection
        (generation, lifecycle_state, aggregate_updated_at DESC, holding_id DESC);

CREATE TABLE erpx_platform_administration_read.connector_projection (
    projection_name TEXT NOT NULL DEFAULT 'platform_administration.connectors'
        CHECK (projection_name = 'platform_administration.connectors'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    connector_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    connector_kind TEXT NOT NULL,
    connector_key TEXT NOT NULL,
    configuration_digest TEXT NOT NULL,
    capabilities JSONB NOT NULL,
    connector_state TEXT NOT NULL CHECK (connector_state IN (
        'pending', 'active', 'suspended', 'retired'
    )),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, connector_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, scope_key, connector_key)
);

CREATE INDEX platform_admin_read_connector_scope
    ON erpx_platform_administration_read.connector_projection
        (generation, scope_key, connector_state, aggregate_updated_at DESC, connector_id DESC);

CREATE TABLE erpx_platform_administration_read.admin_context_projection (
    projection_name TEXT NOT NULL DEFAULT 'platform_administration.admin_contexts'
        CHECK (projection_name = 'platform_administration.admin_contexts'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    context_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_id UUID NOT NULL,
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    principal_ref TEXT NOT NULL,
    reason TEXT NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    context_status TEXT NOT NULL CHECK (context_status IN ('active', 'revoked', 'expired')),
    state JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, context_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    CHECK (expires_at > issued_at)
);

CREATE INDEX platform_admin_read_context_expiry
    ON erpx_platform_administration_read.admin_context_projection
        (generation, holding_id, scope_key, context_status, expires_at, context_id);

CREATE SCHEMA IF NOT EXISTS erpx_brand_read;

CREATE TABLE erpx_brand_read.profile_projection (
    projection_name TEXT NOT NULL DEFAULT 'brand.profiles'
        CHECK (projection_name = 'brand.profiles'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    profile_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    draft_revision BIGINT CHECK (draft_revision > 0),
    published_revision BIGINT CHECK (published_revision > 0),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, profile_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, scope_key)
);

CREATE INDEX brand_read_scope_revision
    ON erpx_brand_read.profile_projection
        (generation, scope_key, published_revision DESC, profile_id DESC);

CREATE TABLE erpx_brand_read.resolved_projection (
    projection_name TEXT NOT NULL DEFAULT 'brand.resolved'
        CHECK (projection_name = 'brand.resolved'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    scope_key TEXT NOT NULL,
    published_revision BIGINT NOT NULL CHECK (published_revision > 0),
    definition JSONB NOT NULL,
    inherited_categories JSONB NOT NULL,
    effective_locked_categories JSONB NOT NULL,
    digest TEXT NOT NULL,
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, scope_key),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE
);

CREATE TABLE erpx_brand_read.history_projection (
    projection_name TEXT NOT NULL DEFAULT 'brand.history'
        CHECK (projection_name = 'brand.history'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    scope_key TEXT NOT NULL,
    profile_id UUID NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    lifecycle TEXT NOT NULL CHECK (lifecycle IN ('draft', 'published')),
    definition JSONB NOT NULL,
    parent_published_revision BIGINT CHECK (parent_published_revision > 0),
    updated_by TEXT NOT NULL,
    reason TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    published_at TIMESTAMPTZ,
    archived_at TIMESTAMPTZ,
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, profile_id, revision),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE
);

CREATE INDEX brand_read_history
    ON erpx_brand_read.history_projection
        (generation, scope_key, revision DESC, profile_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_customer_authority_read;

CREATE TABLE erpx_customer_authority_read.customer_projection (
    projection_name TEXT NOT NULL DEFAULT 'customer_authority.customers'
        CHECK (projection_name = 'customer_authority.customers'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    customer_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    customer_code TEXT NOT NULL,
    trading_name TEXT NOT NULL,
    legal_name TEXT,
    country TEXT NOT NULL CHECK (country ~ '^[A-Z]{2}$'),
    customer_status TEXT NOT NULL CHECK (customer_status IN (
        'prospect', 'onboarding', 'active', 'suspended', 'churned'
    )),
    primary_authority JSONB NOT NULL,
    primary_scope_key TEXT NOT NULL,
    merged_into_customer_id UUID,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, customer_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, customer_code)
);

CREATE INDEX customer_authority_read_search
    ON erpx_customer_authority_read.customer_projection
        (generation, customer_status, trading_name, customer_code, customer_id);

CREATE TABLE erpx_customer_authority_read.authority_link_projection (
    projection_name TEXT NOT NULL DEFAULT 'customer_authority.customers'
        CHECK (projection_name = 'customer_authority.customers'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    customer_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    authority_scope JSONB NOT NULL,
    is_primary BOOLEAN NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, customer_id, scope_key),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    FOREIGN KEY (projection_name, generation, partition_key, customer_id)
        REFERENCES erpx_customer_authority_read.customer_projection
            (projection_name, generation, partition_key, customer_id)
        ON DELETE CASCADE
);

CREATE UNIQUE INDEX customer_authority_read_one_primary
    ON erpx_customer_authority_read.authority_link_projection
        (generation, partition_key, customer_id)
    WHERE is_primary;

CREATE INDEX customer_authority_read_scope
    ON erpx_customer_authority_read.authority_link_projection
        (generation, scope_key, customer_id);

-- D13: disposable active-generation Platform governance read model.
CREATE TABLE erpx_platform_administration_read.governance_record_projection (
    projection_name TEXT NOT NULL DEFAULT 'platform_administration.governance-records'
        CHECK (projection_name = 'platform_administration.governance-records'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (record_type IN (
        'platform.data-plane-operation', 'platform.migration', 'platform.release'
    )),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    record_scope JSONB NOT NULL,
    record_title TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, record_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE
);

CREATE INDEX platform_governance_read_state
    ON erpx_platform_administration_read.governance_record_projection
        (generation, record_type, lifecycle_state, aggregate_updated_at DESC, record_id DESC);


-- Wave 9 composed Customer 360 read model. Customer Authority is read from the
-- control outbox; holding activity uses one independently fenced cursor per
-- immutable holding database route.

CREATE TABLE erpx_cqrs_read.projection_source_checkpoints (
    projection_name TEXT NOT NULL,
    generation UUID NOT NULL,
    source_key TEXT NOT NULL CHECK (octet_length(source_key) BETWEEN 1 AND 255),
    source_sequence BIGINT NOT NULL DEFAULT 0 CHECK (source_sequence >= 0),
    rebuild_target_sequence BIGINT CHECK (rebuild_target_sequence >= 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, source_key),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    CHECK (updated_at >= created_at)
);

CREATE INDEX cqrs_control_projection_source_rebuild
    ON erpx_cqrs_read.projection_source_checkpoints
        (projection_name, generation, rebuild_target_sequence, source_sequence);

CREATE SCHEMA IF NOT EXISTS erpx_customer360_read;

CREATE TABLE erpx_customer360_read.customer_profile_projection (
    projection_name TEXT NOT NULL DEFAULT 'customer360.control_customers'
        CHECK (projection_name = 'customer360.control_customers'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    customer_id UUID NOT NULL,
    customer_account_id UUID NOT NULL,
    customer_code TEXT NOT NULL,
    display_name TEXT NOT NULL,
    owning_company_id UUID NOT NULL,
    customer_status TEXT NOT NULL CHECK (customer_status IN (
        'prospect', 'active', 'dormant', 'blocked', 'closed'
    )),
    source_event_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    occurred_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, customer_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, customer_account_id),
    UNIQUE (generation, customer_code)
);

CREATE INDEX customer360_profile_customer
    ON erpx_customer360_read.customer_profile_projection
        (generation, customer_id, projected_at DESC);

CREATE TABLE erpx_customer360_read.subscription_projection (
    projection_name TEXT NOT NULL DEFAULT 'customer360.holding_activity'
        CHECK (projection_name = 'customer360.holding_activity'),
    generation UUID NOT NULL,
    source_key TEXT NOT NULL,
    partition_key TEXT NOT NULL,
    holding_id UUID NOT NULL,
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    customer_id UUID NOT NULL,
    subscription_id UUID NOT NULL,
    plan_id UUID NOT NULL,
    subscription_status TEXT NOT NULL CHECK (subscription_status IN (
        'trial', 'active', 'past_due', 'suspended', 'ended'
    )),
    recurring_amount_minor BIGINT NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    effective_at TIMESTAMPTZ NOT NULL,
    source_event_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, source_key, subscription_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, source_key, partition_key, source_event_id),
    CHECK (source_key = 'holding:' || holding_id::text)
);

CREATE INDEX customer360_subscription_customer
    ON erpx_customer360_read.subscription_projection
        (generation, customer_id, source_key, effective_at DESC, subscription_id DESC);

CREATE TABLE erpx_customer360_read.opportunity_projection (
    projection_name TEXT NOT NULL DEFAULT 'customer360.holding_activity'
        CHECK (projection_name = 'customer360.holding_activity'),
    generation UUID NOT NULL,
    source_key TEXT NOT NULL,
    partition_key TEXT NOT NULL,
    holding_id UUID NOT NULL,
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    customer_id UUID NOT NULL,
    opportunity_id UUID NOT NULL,
    opportunity_stage TEXT NOT NULL CHECK (opportunity_stage IN (
        'qualified', 'discovery', 'proposal', 'negotiation', 'won', 'lost'
    )),
    expected_value_minor BIGINT NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    source_event_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, source_key, opportunity_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, source_key, partition_key, source_event_id),
    CHECK (source_key = 'holding:' || holding_id::text)
);

CREATE INDEX customer360_opportunity_customer
    ON erpx_customer360_read.opportunity_projection
        (generation, customer_id, source_key, opportunity_id DESC);

CREATE TABLE erpx_customer360_read.sales_order_projection (
    projection_name TEXT NOT NULL DEFAULT 'customer360.holding_activity'
        CHECK (projection_name = 'customer360.holding_activity'),
    generation UUID NOT NULL,
    source_key TEXT NOT NULL,
    partition_key TEXT NOT NULL,
    holding_id UUID NOT NULL,
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    customer_id UUID NOT NULL,
    sales_order_id UUID NOT NULL,
    order_code TEXT NOT NULL,
    order_status TEXT NOT NULL CHECK (order_status IN (
        'confirmed', 'partially_fulfilled', 'fulfilled', 'cancelled'
    )),
    total_minor BIGINT NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    lines JSONB NOT NULL CHECK (
        jsonb_typeof(lines) = 'array' AND jsonb_array_length(lines) > 0
    ),
    source_event_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, source_key, sales_order_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, source_key, partition_key, source_event_id),
    UNIQUE (generation, source_key, order_code),
    CHECK (source_key = 'holding:' || holding_id::text)
);

CREATE INDEX customer360_sales_order_customer
    ON erpx_customer360_read.sales_order_projection
        (generation, customer_id, source_key, sales_order_id DESC);

CREATE TABLE erpx_customer360_read.support_case_projection (
    projection_name TEXT NOT NULL DEFAULT 'customer360.holding_activity'
        CHECK (projection_name = 'customer360.holding_activity'),
    generation UUID NOT NULL,
    source_key TEXT NOT NULL,
    partition_key TEXT NOT NULL,
    holding_id UUID NOT NULL,
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    customer_id UUID NOT NULL,
    support_case_id UUID NOT NULL,
    case_code TEXT NOT NULL,
    case_status TEXT NOT NULL CHECK (case_status IN (
        'open', 'pending_customer', 'pending_internal', 'resolved', 'closed'
    )),
    category TEXT NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    source_event_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, source_key, support_case_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, source_key, partition_key, source_event_id),
    UNIQUE (generation, source_key, case_code),
    CHECK (source_key = 'holding:' || holding_id::text)
);

CREATE INDEX customer360_support_case_customer
    ON erpx_customer360_read.support_case_projection
        (generation, customer_id, source_key, aggregate_updated_at DESC, support_case_id DESC);


-- Disposable control-read database generation marker.
CREATE SCHEMA erpx_meta;

CREATE TABLE erpx_meta.schema_manifest (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    schema_revision SMALLINT NOT NULL CHECK (schema_revision = 2),
    manifest_version SMALLINT NOT NULL CHECK (manifest_version = 1),
    database_role TEXT NOT NULL CHECK (database_role = 'control_read'),
    migration_set TEXT NOT NULL CHECK (migration_set = 'control-read'),
    installed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO erpx_meta.schema_manifest
    (singleton, schema_revision, manifest_version, database_role, migration_set)
VALUES (TRUE, 2, 1, 'control_read', 'control-read');


-- The read database is disposable, but refuse to silently hide a legacy n8n
-- projection or projection failure/receipt trail during an in-place upgrade.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM erpx_platform_administration_read.connector_projection
        WHERE lower(connector_kind) = 'n8n' OR lower(state::text) LIKE '%n8n%'
    ) OR EXISTS (
        SELECT 1 FROM erpx_cqrs_read.projection_failures
        WHERE projection_name = 'platform_administration.connectors'
          AND lower(last_error) LIKE '%n8n%'
    ) THEN
        RAISE EXCEPTION 'n8n connector projection evidence exists; retirement requires explicit reconciliation';
    END IF;
END;
$$;

ALTER TABLE erpx_platform_administration_read.connector_projection
    ADD CONSTRAINT platform_connector_projection_n8n_retired CHECK (
        lower(connector_kind) <> 'n8n' AND lower(state::text) NOT LIKE '%n8n%'
    );

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM erpx_meta.schema_manifest
        WHERE singleton=TRUE AND schema_revision=2 AND manifest_version=1
          AND database_role='control_read' AND migration_set='control-read'
    ) THEN
        RAISE EXCEPTION 'control-read current manifest is not current';
    END IF;
END;
$$;
