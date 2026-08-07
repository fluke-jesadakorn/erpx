-- Wave 9 disposable Operations projections and topology-neutral per-source
-- cursors. A source cursor is never compared with a cursor from another DB.

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

CREATE INDEX cqrs_tenant_projection_source_rebuild
    ON erpx_cqrs_read.projection_source_checkpoints
        (projection_name, generation, rebuild_target_sequence, source_sequence);

CREATE SCHEMA IF NOT EXISTS erpx_operations_read;

CREATE FUNCTION erpx_operations_read.valid_source_context_array(values_to_check JSONB)
RETURNS BOOLEAN
LANGUAGE sql
IMMUTABLE
STRICT
AS $$
    SELECT CASE
        WHEN jsonb_typeof(values_to_check) <> 'array' THEN FALSE
        ELSE (
            SELECT count(*) BETWEEN 1 AND 16
               AND count(*) = count(DISTINCT value)
               AND bool_and(value #>> '{}' = ANY (ARRAY[
                   'commercial', 'customer', 'marketing', 'crm', 'procurement',
                   'inventory', 'assets', 'maintenance', 'quality', 'sales',
                   'people_payroll', 'pos', 'finance', 'support'
               ]::TEXT[]))
            FROM jsonb_array_elements(values_to_check) AS value
        )
    END
$$;

CREATE TABLE erpx_operations_read.source_signal_projection (
    projection_name TEXT NOT NULL DEFAULT 'operations.source_signals'
        CHECK (projection_name = 'operations.source_signals'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    process_id UUID NOT NULL,
    source_context TEXT NOT NULL CHECK (source_context IN (
        'commercial', 'customer', 'marketing', 'crm', 'procurement', 'inventory',
        'assets', 'maintenance', 'quality', 'sales', 'people_payroll', 'pos',
        'finance', 'support'
    )),
    source_event_id UUID NOT NULL,
    source_event_type TEXT NOT NULL,
    source_event_version INTEGER NOT NULL CHECK (source_event_version > 0),
    source_aggregate_type TEXT NOT NULL,
    source_aggregate_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    process_code TEXT NOT NULL,
    operational_state TEXT NOT NULL,
    source_fact JSONB NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, process_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, source_context, source_event_id)
);

CREATE INDEX operations_source_signal_scope_context
    ON erpx_operations_read.source_signal_projection
        (generation, scope_key, source_context, observed_at DESC, process_id DESC);

CREATE TABLE erpx_operations_read.exception_projection (
    projection_name TEXT NOT NULL DEFAULT 'operations.exceptions'
        CHECK (projection_name = 'operations.exceptions'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    exception_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    reference TEXT NOT NULL,
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    source_context TEXT NOT NULL CHECK (source_context IN (
        'commercial', 'customer', 'marketing', 'crm', 'procurement', 'inventory',
        'assets', 'maintenance', 'quality', 'sales', 'people_payroll', 'pos',
        'finance', 'support'
    )),
    source_event_id UUID NOT NULL,
    source_event_type TEXT NOT NULL,
    source_aggregate_type TEXT NOT NULL,
    source_aggregate_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    source_correlation_id UUID NOT NULL,
    policy_ref TEXT NOT NULL,
    category TEXT NOT NULL,
    severity TEXT NOT NULL,
    assignee_id TEXT NOT NULL,
    manager_id TEXT NOT NULL,
    due_at TIMESTAMPTZ NOT NULL,
    escalation_after_minutes INTEGER NOT NULL,
    exception_status TEXT NOT NULL CHECK (exception_status IN (
        'assigned', 'acknowledged', 'escalated', 'resolved', 'closed'
    )),
    state JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, exception_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, source_context, source_event_id)
);

CREATE INDEX operations_exception_read_queue
    ON erpx_operations_read.exception_projection
        (generation, scope_key, exception_status, updated_at DESC, exception_id DESC);
CREATE INDEX operations_exception_read_assignee
    ON erpx_operations_read.exception_projection
        (generation, scope_key, assignee_id, updated_at DESC, exception_id DESC);
CREATE INDEX operations_exception_read_due
    ON erpx_operations_read.exception_projection
        (generation, scope_key, due_at, exception_id);
CREATE INDEX operations_exception_read_scope_cursor
    ON erpx_operations_read.exception_projection
        (generation, scope_key, exception_id);
CREATE INDEX operations_exception_read_platform_cursor
    ON erpx_operations_read.exception_projection
        (generation, exception_id);

CREATE TABLE erpx_operations_read.work_queue_policy_projection (
    projection_name TEXT NOT NULL DEFAULT 'operations.work_queue_policies'
        CHECK (projection_name = 'operations.work_queue_policies'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    policy_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    policy_name TEXT NOT NULL,
    allowed_source_contexts JSONB NOT NULL CHECK (
        jsonb_typeof(allowed_source_contexts) = 'array'
        AND jsonb_array_length(allowed_source_contexts) BETWEEN 1 AND 16
        AND erpx_operations_read.valid_source_context_array(allowed_source_contexts)
    ),
    escalation_after_minutes INTEGER NOT NULL,
    policy_status TEXT NOT NULL CHECK (policy_status IN ('active', 'paused', 'retired')),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    state JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, policy_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, scope_key, policy_name)
);

CREATE INDEX operations_policy_read_scope_status
    ON erpx_operations_read.work_queue_policy_projection
        (generation, scope_key, policy_status, updated_at DESC, policy_id DESC);
CREATE INDEX operations_policy_read_scope_cursor
    ON erpx_operations_read.work_queue_policy_projection
        (generation, scope_key, policy_id);
CREATE INDEX operations_policy_read_platform_cursor
    ON erpx_operations_read.work_queue_policy_projection
        (generation, policy_id);
