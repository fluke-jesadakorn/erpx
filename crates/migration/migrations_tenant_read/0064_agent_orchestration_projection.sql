-- Disposable authorized Agent Orchestration read projections. Prompt and
-- output material remain encrypted references in the write plane and never
-- enter these cross-workspace listings.

CREATE SCHEMA IF NOT EXISTS erpx_agent_read;

CREATE OR REPLACE FUNCTION erpx_agent_read.current_tenant_id()
RETURNS UUID LANGUAGE SQL STABLE AS $$
    SELECT nullif(current_setting('app.tenant_id', true), '')::uuid
$$;

CREATE TABLE erpx_agent_read.agent_directory_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'agent_orchestration.agent_directory'
        CHECK (projection_name = 'agent_orchestration.agent_directory'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    agent_definition_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    scope_key TEXT NOT NULL,
    tenant_scope JSONB NOT NULL,
    agent_principal_ref TEXT NOT NULL,
    agent_name TEXT NOT NULL,
    definition_status TEXT NOT NULL CHECK (definition_status IN (
        'draft', 'approved', 'suspended', 'retired'
    )),
    approved_version_ids UUID[] NOT NULL,
    capabilities TEXT[] NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, agent_definition_id),
    FOREIGN KEY (tenant_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (tenant_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE INDEX agent_directory_scope_status
    ON erpx_agent_read.agent_directory_projection
        (tenant_id, generation, scope_key, definition_status, agent_name, agent_definition_id);

CREATE TABLE erpx_agent_read.assignment_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'agent_orchestration.assignments'
        CHECK (projection_name = 'agent_orchestration.assignments'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    assignment_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    scope_key TEXT NOT NULL,
    tenant_scope JSONB NOT NULL,
    root_assignment_id UUID NOT NULL,
    parent_assignment_id UUID,
    initiating_principal_ref TEXT NOT NULL,
    agent_principal_ref TEXT NOT NULL,
    agent_definition_id UUID NOT NULL,
    agent_version_id UUID NOT NULL,
    provider_ref TEXT NOT NULL,
    model_ref TEXT NOT NULL,
    policy_digest TEXT NOT NULL CHECK (policy_digest ~ '^[0-9A-Fa-f]{64}$'),
    assignment_status TEXT NOT NULL CHECK (assignment_status IN (
        'queued', 'running', 'waiting', 'completed', 'failed', 'cancelled'
    )),
    attempt INTEGER NOT NULL CHECK (attempt >= 0),
    depth INTEGER NOT NULL CHECK (depth >= 0),
    queue_priority INTEGER NOT NULL CHECK (queue_priority BETWEEN 0 AND 65535),
    estimated_units BIGINT NOT NULL CHECK (estimated_units > 0),
    estimated_cost_micros BIGINT NOT NULL CHECK (estimated_cost_micros >= 0),
    actual_units BIGINT CHECK (actual_units >= 0),
    actual_cost_micros BIGINT CHECK (actual_cost_micros >= 0),
    failure_code TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, assignment_id),
    FOREIGN KEY (tenant_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (tenant_id, projection_name, generation)
        ON DELETE CASCADE,
    CHECK (updated_at >= created_at)
);

CREATE INDEX agent_assignment_inbox
    ON erpx_agent_read.assignment_projection
        (tenant_id, generation, scope_key, initiating_principal_ref, assignment_status,
         updated_at DESC, assignment_id DESC);

CREATE TABLE erpx_agent_read.private_session_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'agent_orchestration.private_sessions'
        CHECK (projection_name = 'agent_orchestration.private_sessions'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    agent_session_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    scope_key TEXT NOT NULL,
    owner_human_ref TEXT NOT NULL,
    record_id UUID,
    expires_at TIMESTAMPTZ NOT NULL,
    legal_hold BOOLEAN NOT NULL,
    expired_at TIMESTAMPTZ,
    message_count INTEGER NOT NULL CHECK (message_count >= 0),
    published_message_count INTEGER NOT NULL CHECK (
        published_message_count BETWEEN 0 AND message_count
    ),
    updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, agent_session_id),
    FOREIGN KEY (tenant_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (tenant_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE INDEX agent_private_sessions_owner
    ON erpx_agent_read.private_session_projection
        (tenant_id, generation, scope_key, owner_human_ref, updated_at DESC, agent_session_id DESC);

CREATE TABLE erpx_agent_read.pool_directory_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'agent_orchestration.pools'
        CHECK (projection_name = 'agent_orchestration.pools'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    agent_pool_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    scope_key TEXT NOT NULL,
    pool_name TEXT NOT NULL,
    member_version_ids UUID[] NOT NULL,
    enabled BOOLEAN NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, agent_pool_id),
    FOREIGN KEY (tenant_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (tenant_id, projection_name, generation)
        ON DELETE CASCADE,
    CHECK (cardinality(member_version_ids) > 0)
);

CREATE INDEX agent_pool_directory_scope
    ON erpx_agent_read.pool_directory_projection
        (tenant_id, generation, scope_key, enabled, pool_name, agent_pool_id);

CREATE TABLE erpx_agent_read.pool_routing_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'agent_orchestration.pool_routing'
        CHECK (projection_name = 'agent_orchestration.pool_routing'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    routing_decision_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    scope_key TEXT NOT NULL,
    requested_scope JSONB NOT NULL,
    agent_pool_id UUID NOT NULL,
    root_action_ref TEXT NOT NULL,
    capability TEXT NOT NULL,
    selected_agent_version_ids UUID[] NOT NULL,
    compare_requested BOOLEAN NOT NULL,
    decision_digest TEXT NOT NULL CHECK (decision_digest ~ '^[0-9A-Fa-f]{64}$'),
    decided_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, routing_decision_id),
    FOREIGN KEY (tenant_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (tenant_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE TABLE erpx_agent_read.provider_invocation_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'agent_orchestration.provider_invocations'
        CHECK (projection_name = 'agent_orchestration.provider_invocations'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    invocation_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    scope_key TEXT NOT NULL,
    tenant_scope JSONB NOT NULL,
    assignment_id UUID NOT NULL,
    attempt INTEGER NOT NULL CHECK (attempt > 0),
    provider_ref TEXT NOT NULL,
    model_ref TEXT NOT NULL,
    policy_version BIGINT NOT NULL CHECK (policy_version > 0),
    policy_digest TEXT NOT NULL CHECK (policy_digest ~ '^[0-9A-Fa-f]{64}$'),
    payload_digest TEXT NOT NULL CHECK (payload_digest ~ '^[0-9A-Fa-f]{64}$'),
    output_digest TEXT CHECK (output_digest ~ '^[0-9A-Fa-f]{64}$'),
    provider_units BIGINT CHECK (provider_units > 0),
    provider_cost_micros BIGINT CHECK (provider_cost_micros >= 0),
    outcome TEXT CHECK (outcome IN ('completed', 'failed', 'cancelled')),
    invoked_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, invocation_id),
    FOREIGN KEY (tenant_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (tenant_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE TABLE erpx_agent_read.action_card_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'agent_orchestration.action_cards'
        CHECK (projection_name = 'agent_orchestration.action_cards'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    action_card_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    scope_key TEXT NOT NULL,
    assignment_id UUID NOT NULL,
    agent_principal_ref TEXT NOT NULL,
    owner_context TEXT NOT NULL,
    command_type TEXT NOT NULL,
    action_status TEXT NOT NULL CHECK (action_status IN (
        'proposed', 'rejected', 'confirmed_for_handoff'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    resolved_at TIMESTAMPTZ,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, action_card_id),
    FOREIGN KEY (tenant_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (tenant_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE TABLE erpx_agent_read.governance_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'agent_orchestration.governance'
        CHECK (projection_name = 'agent_orchestration.governance'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    scope_key TEXT NOT NULL,
    tenant_scope JSONB NOT NULL,
    record_kind TEXT NOT NULL CHECK (record_kind IN ('delegation_tree','budget_decision')),
    subject_record_id UUID NOT NULL,
    policy_digest TEXT NOT NULL CHECK (policy_digest ~ '^[0-9A-Fa-f]{64}$'),
    lifecycle_state TEXT NOT NULL CHECK (lifecycle_state IN (
        'proposed','approved','active','completed','denied','expired','cancelled'
    )),
    updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, record_id),
    UNIQUE (tenant_id, projection_name, generation, record_id),
    FOREIGN KEY (tenant_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (tenant_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE INDEX agent_governance_scope_state
    ON erpx_agent_read.governance_projection
        (tenant_id, generation, scope_key, record_kind, lifecycle_state, updated_at DESC, record_id);

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'agent_directory_projection','assignment_projection',
        'private_session_projection','pool_directory_projection',
        'pool_routing_projection','provider_invocation_projection',
        'action_card_projection','governance_projection'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_agent_read.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_agent_read.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY agent_read_tenant_isolation ON erpx_agent_read.%I '
            'USING (tenant_id = erpx_agent_read.current_tenant_id()) '
            'WITH CHECK (tenant_id = erpx_agent_read.current_tenant_id())',
            table_name
        );
    END LOOP;
END;
$$;
