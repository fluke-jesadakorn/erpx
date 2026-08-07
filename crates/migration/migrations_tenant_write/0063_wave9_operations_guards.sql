-- Wave 9 Operations authority. Generic CQRS snapshots, receipts, audit rows,
-- and outbox events remain authoritative and are committed in the same SQL
-- transaction as these searchable relational guards.

CREATE SCHEMA IF NOT EXISTS erpx_operations_write;

CREATE TABLE erpx_operations_write.exception_guards (
    exception_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    reference TEXT NOT NULL,
    tenant_scope JSONB NOT NULL,
    category TEXT NOT NULL,
    severity TEXT NOT NULL,
    source_context TEXT NOT NULL CHECK (source_context IN (
        'commercial', 'customer', 'marketing', 'crm', 'procurement', 'inventory',
        'assets', 'maintenance', 'quality', 'sales', 'people_payroll', 'pos',
        'finance', 'support'
    )),
    source_event_id UUID NOT NULL,
    source_event_type TEXT NOT NULL,
    source_event_version INTEGER NOT NULL CHECK (source_event_version > 0),
    source_partition_key TEXT NOT NULL,
    source_aggregate_type TEXT NOT NULL,
    source_aggregate_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    source_correlation_id UUID NOT NULL,
    policy_ref TEXT NOT NULL,
    assignee_id TEXT NOT NULL,
    manager_id TEXT NOT NULL,
    assignment_authority_ref TEXT NOT NULL,
    assignment_valid_from TIMESTAMPTZ NOT NULL,
    assignment_valid_until TIMESTAMPTZ NOT NULL,
    due_at TIMESTAMPTZ NOT NULL,
    escalation_after_minutes INTEGER NOT NULL CHECK (
        escalation_after_minutes BETWEEN 1 AND 10080
    ),
    exception_status TEXT NOT NULL CHECK (exception_status IN (
        'assigned', 'acknowledged', 'escalated', 'resolved', 'closed'
    )),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (updated_at >= created_at),
    CHECK (assignment_valid_until > assignment_valid_from),
    CHECK (assignee_id <> manager_id),
    UNIQUE (scope_key, reference),
    UNIQUE (source_context, source_event_id),
    UNIQUE (exception_id, source_context, source_event_id)
);

CREATE INDEX operations_exception_scope_status
    ON erpx_operations_write.exception_guards
        (scope_key, exception_status, updated_at DESC, exception_id DESC);
CREATE INDEX operations_exception_source_aggregate
    ON erpx_operations_write.exception_guards
        (scope_key, source_context, source_aggregate_id);

CREATE TABLE erpx_operations_write.source_receipts (
    source_event_id UUID PRIMARY KEY,
    source_context TEXT NOT NULL,
    source_fingerprint JSONB NOT NULL,
    disposition TEXT NOT NULL CHECK (disposition IN ('ignored', 'detected')),
    exception_id UUID,
    applied_at TIMESTAMPTZ NOT NULL,
    CHECK ((disposition = 'ignored') = (exception_id IS NULL)),
    FOREIGN KEY (exception_id, source_context, source_event_id)
        REFERENCES erpx_operations_write.exception_guards
            (exception_id, source_context, source_event_id)
);

CREATE UNIQUE INDEX operations_source_receipt_identity
    ON erpx_operations_write.source_receipts (source_context, source_event_id);

CREATE OR REPLACE FUNCTION erpx_operations_write.reject_source_receipt_mutation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'Operations source receipts are immutable';
END
$$;

CREATE TRIGGER operations_source_receipt_immutable
    BEFORE UPDATE OR DELETE ON erpx_operations_write.source_receipts
    FOR EACH ROW EXECUTE FUNCTION erpx_operations_write.reject_source_receipt_mutation();

CREATE TABLE erpx_operations_write.source_ingestion_checkpoints (
    source_database TEXT NOT NULL CHECK (source_database IN ('control', 'tenant')),
    source_key TEXT NOT NULL CHECK (octet_length(source_key) BETWEEN 1 AND 255),
    source_partition TEXT NOT NULL,
    source_sequence BIGINT NOT NULL DEFAULT 0 CHECK (source_sequence >= 0),
    source_event_id UUID,
    source_occurred_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (source_database, source_key, source_partition),
    CHECK ((source_sequence = 0) = (source_event_id IS NULL))
);

CREATE FUNCTION erpx_operations_write.valid_source_context_array(values_to_check JSONB)
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

CREATE TABLE erpx_operations_write.work_queue_policy_guards (
    policy_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    tenant_scope JSONB NOT NULL,
    policy_name TEXT NOT NULL,
    allowed_source_contexts JSONB NOT NULL CHECK (
        jsonb_typeof(allowed_source_contexts) = 'array'
        AND jsonb_array_length(allowed_source_contexts) BETWEEN 1 AND 16
        AND erpx_operations_write.valid_source_context_array(allowed_source_contexts)
    ),
    escalation_after_minutes INTEGER NOT NULL CHECK (
        escalation_after_minutes BETWEEN 1 AND 10080
    ),
    policy_status TEXT NOT NULL CHECK (policy_status IN ('active', 'paused', 'retired')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (updated_at >= created_at),
    UNIQUE (scope_key, policy_name)
);

CREATE INDEX operations_policy_scope_status
    ON erpx_operations_write.work_queue_policy_guards
        (scope_key, policy_status, updated_at DESC, policy_id DESC);
