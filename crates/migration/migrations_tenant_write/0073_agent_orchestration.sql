-- Agent Orchestration owns governed agent versions, durable assignments,
-- delegation lineage, private sessions, and budget/usage evidence. It never
-- owns or directly mutates records in another business context.

CREATE SCHEMA IF NOT EXISTS erpx_agent_write;

-- Every row is bound to the tenant selected by the adapter transaction. A
-- missing tenant setting evaluates to NULL, so inserts and RLS checks fail
-- closed instead of falling back to an unscoped data plane.
CREATE OR REPLACE FUNCTION erpx_agent_write.current_tenant_id()
RETURNS UUID LANGUAGE SQL STABLE AS $$
    SELECT nullif(current_setting('app.tenant_id', true), '')::uuid
$$;

CREATE TABLE erpx_agent_write.agent_definitions (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    agent_definition_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    tenant_scope JSONB NOT NULL,
    agent_principal_ref TEXT NOT NULL,
    business_owner_ref TEXT NOT NULL,
    technical_owner_ref TEXT NOT NULL,
    agent_name TEXT NOT NULL,
    definition_status TEXT NOT NULL CHECK (definition_status IN (
        'draft', 'approved', 'suspended', 'retired'
    )),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, agent_definition_id),
    UNIQUE (tenant_id, scope_key, agent_principal_ref),
    CHECK (updated_at >= created_at)
);

CREATE TABLE erpx_agent_write.agent_versions (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    agent_version_id UUID NOT NULL,
    agent_definition_id UUID NOT NULL,
    version_number INTEGER NOT NULL CHECK (version_number > 0),
    version_status TEXT NOT NULL CHECK (version_status IN (
        'draft', 'approved', 'suspended', 'retired'
    )),
    purpose TEXT NOT NULL,
    provider_ref TEXT NOT NULL,
    model_ref TEXT NOT NULL,
    use_case TEXT NOT NULL,
    prompt_digest TEXT NOT NULL CHECK (prompt_digest ~ '^[0-9A-Fa-f]{64}$'),
    policy_digest TEXT NOT NULL CHECK (policy_digest ~ '^[0-9A-Fa-f]{64}$'),
    maximum_classification TEXT NOT NULL CHECK (maximum_classification IN (
        'public', 'internal', 'confidential', 'restricted'
    )),
    security_review_ref TEXT NOT NULL,
    privacy_review_ref TEXT NOT NULL,
    max_attempts INTEGER NOT NULL CHECK (max_attempts > 0),
    timeout_seconds INTEGER NOT NULL CHECK (timeout_seconds > 0),
    approved_by TEXT,
    approved_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, agent_version_id),
    UNIQUE (tenant_id, agent_definition_id, version_number),
    FOREIGN KEY (tenant_id, agent_definition_id) REFERENCES
        erpx_agent_write.agent_definitions (tenant_id, agent_definition_id) ON DELETE RESTRICT,
    CHECK (
        (version_status = 'approved' AND approved_by IS NOT NULL AND approved_at IS NOT NULL)
        OR version_status <> 'approved'
    )
);

CREATE TABLE erpx_agent_write.agent_version_capabilities (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    agent_version_id UUID NOT NULL,
    capability TEXT NOT NULL,
    PRIMARY KEY (tenant_id, agent_version_id, capability),
    FOREIGN KEY (tenant_id, agent_version_id) REFERENCES
        erpx_agent_write.agent_versions (tenant_id, agent_version_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_agent_write.agent_version_tools (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    agent_version_id UUID NOT NULL,
    tool_ref TEXT NOT NULL,
    PRIMARY KEY (tenant_id, agent_version_id, tool_ref),
    FOREIGN KEY (tenant_id, agent_version_id) REFERENCES
        erpx_agent_write.agent_versions (tenant_id, agent_version_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_agent_write.agent_version_knowledge_scopes (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    agent_version_id UUID NOT NULL,
    knowledge_scope TEXT NOT NULL,
    PRIMARY KEY (tenant_id, agent_version_id, knowledge_scope),
    FOREIGN KEY (tenant_id, agent_version_id) REFERENCES
        erpx_agent_write.agent_versions (tenant_id, agent_version_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_agent_write.agent_version_organization_scopes (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    agent_version_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    tenant_scope JSONB NOT NULL,
    PRIMARY KEY (tenant_id, agent_version_id, scope_key),
    FOREIGN KEY (tenant_id, agent_version_id) REFERENCES
        erpx_agent_write.agent_versions (tenant_id, agent_version_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_agent_write.agent_version_evaluations (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    agent_version_id UUID NOT NULL,
    evaluation_ref TEXT NOT NULL,
    PRIMARY KEY (tenant_id, agent_version_id, evaluation_ref),
    FOREIGN KEY (tenant_id, agent_version_id) REFERENCES
        erpx_agent_write.agent_versions (tenant_id, agent_version_id) ON DELETE RESTRICT
);

-- Provider policy is tenant-owned governance evidence, not provider secret
-- material. A worker accepts only one current approved policy whose digest is
-- pinned by the exact approved agent version and whose freshness observation
-- is still current. Revocation remains append-only evidence.
CREATE TABLE erpx_agent_write.agent_provider_policies (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    provider_policy_id UUID NOT NULL,
    provider_ref TEXT NOT NULL,
    policy_version BIGINT NOT NULL CHECK (policy_version > 0),
    policy_status TEXT NOT NULL CHECK (policy_status IN (
        'draft', 'approved', 'suspended', 'retired'
    )),
    is_current BOOLEAN NOT NULL DEFAULT FALSE,
    external_provider BOOLEAN NOT NULL,
    dpa_ref TEXT,
    maximum_classification TEXT NOT NULL CHECK (maximum_classification IN (
        'public', 'internal', 'confidential', 'restricted'
    )),
    policy_digest TEXT NOT NULL CHECK (policy_digest ~ '^[0-9A-Fa-f]{64}$'),
    effective_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    freshness_checked_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, provider_policy_id),
    UNIQUE (tenant_id, provider_ref, policy_version),
    UNIQUE (tenant_id, provider_ref, policy_digest),
    CHECK (expires_at > effective_at),
    CHECK (freshness_checked_at BETWEEN effective_at AND updated_at),
    CHECK (NOT external_provider OR dpa_ref IS NOT NULL)
);

CREATE UNIQUE INDEX agent_provider_one_current_policy
    ON erpx_agent_write.agent_provider_policies (tenant_id, provider_ref)
    WHERE is_current AND policy_status = 'approved';

CREATE TABLE erpx_agent_write.agent_provider_policy_models (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    provider_policy_id UUID NOT NULL,
    model_ref TEXT NOT NULL,
    PRIMARY KEY (tenant_id, provider_policy_id, model_ref),
    FOREIGN KEY (tenant_id, provider_policy_id) REFERENCES
        erpx_agent_write.agent_provider_policies (tenant_id, provider_policy_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_agent_write.agent_provider_policy_use_cases (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    provider_policy_id UUID NOT NULL,
    use_case TEXT NOT NULL,
    PRIMARY KEY (tenant_id, provider_policy_id, use_case),
    FOREIGN KEY (tenant_id, provider_policy_id) REFERENCES
        erpx_agent_write.agent_provider_policies (tenant_id, provider_policy_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_agent_write.agent_provider_policy_residencies (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    provider_policy_id UUID NOT NULL,
    residency TEXT NOT NULL,
    PRIMARY KEY (tenant_id, provider_policy_id, residency),
    FOREIGN KEY (tenant_id, provider_policy_id) REFERENCES
        erpx_agent_write.agent_provider_policies (tenant_id, provider_policy_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_agent_write.agent_provider_policy_evaluations (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    provider_policy_id UUID NOT NULL,
    agent_version_id UUID NOT NULL,
    evaluation_ref TEXT NOT NULL,
    decision_status TEXT NOT NULL CHECK (decision_status IN ('approved', 'revoked')),
    evidence_digest TEXT NOT NULL CHECK (evidence_digest ~ '^[0-9A-Fa-f]{64}$'),
    decided_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, provider_policy_id, agent_version_id, evaluation_ref),
    FOREIGN KEY (tenant_id, provider_policy_id) REFERENCES
        erpx_agent_write.agent_provider_policies (tenant_id, provider_policy_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, agent_version_id) REFERENCES
        erpx_agent_write.agent_versions (tenant_id, agent_version_id) ON DELETE RESTRICT,
    CHECK (expires_at > decided_at)
);

CREATE TABLE erpx_agent_write.agent_provider_policy_revocations (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    provider_policy_id UUID NOT NULL,
    policy_version BIGINT NOT NULL CHECK (policy_version > 0),
    reason_digest TEXT NOT NULL CHECK (reason_digest ~ '^[0-9A-Fa-f]{64}$'),
    revoked_by TEXT NOT NULL,
    revoked_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, provider_policy_id),
    FOREIGN KEY (tenant_id, provider_policy_id) REFERENCES
        erpx_agent_write.agent_provider_policies (tenant_id, provider_policy_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_agent_write.agent_material_retention_policies (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    use_case TEXT NOT NULL,
    policy_version BIGINT NOT NULL CHECK (policy_version > 0),
    policy_status TEXT NOT NULL CHECK (policy_status IN ('approved', 'suspended', 'retired')),
    ttl_seconds BIGINT NOT NULL CHECK (ttl_seconds BETWEEN 60 AND 7776000),
    legal_hold_default BOOLEAN NOT NULL DEFAULT FALSE,
    effective_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    freshness_checked_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, use_case, policy_version),
    CHECK (expires_at > effective_at),
    CHECK (freshness_checked_at BETWEEN effective_at AND updated_at)
);

CREATE UNIQUE INDEX agent_material_one_current_retention
    ON erpx_agent_write.agent_material_retention_policies (tenant_id, use_case)
    WHERE policy_status = 'approved';

CREATE TABLE erpx_agent_write.agent_budget_limits (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    layer_kind TEXT NOT NULL CHECK (layer_kind IN (
        'tenant', 'workspace', 'agent', 'principal', 'root_assignment'
    )),
    subject_ref TEXT NOT NULL,
    remaining_units BIGINT NOT NULL CHECK (remaining_units >= 0),
    remaining_cost_micros BIGINT NOT NULL CHECK (remaining_cost_micros >= 0),
    active_concurrency INTEGER NOT NULL CHECK (active_concurrency >= 0),
    concurrency_limit INTEGER NOT NULL CHECK (concurrency_limit > 0),
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    policy_version BIGINT NOT NULL CHECK (policy_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, layer_kind, subject_ref),
    CHECK (active_concurrency <= concurrency_limit)
);

CREATE TABLE erpx_agent_write.agent_budget_reservations (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    reservation_id UUID NOT NULL,
    assignment_id UUID,
    estimated_units BIGINT NOT NULL CHECK (estimated_units > 0),
    estimated_cost_micros BIGINT NOT NULL CHECK (estimated_cost_micros >= 0),
    stop_after_units BIGINT NOT NULL CHECK (stop_after_units >= estimated_units),
    queue_priority INTEGER NOT NULL CHECK (queue_priority BETWEEN 0 AND 65535),
    reservation_status TEXT NOT NULL CHECK (reservation_status IN (
        'reserved', 'reconciled', 'over_budget', 'released', 'cancelled'
    )),
    actual_units BIGINT CHECK (actual_units >= 0),
    actual_cost_micros BIGINT CHECK (actual_cost_micros >= 0),
    reserved_at TIMESTAMPTZ NOT NULL,
    reconciled_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, reservation_id),
    UNIQUE (tenant_id, assignment_id)
);

CREATE TABLE erpx_agent_write.agent_budget_reservation_layers (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    reservation_id UUID NOT NULL,
    layer_kind TEXT NOT NULL CHECK (layer_kind IN (
        'tenant', 'workspace', 'agent', 'principal', 'root_assignment'
    )),
    subject_ref TEXT NOT NULL,
    reserved_units BIGINT NOT NULL CHECK (reserved_units > 0),
    reserved_cost_micros BIGINT NOT NULL CHECK (reserved_cost_micros >= 0),
    PRIMARY KEY (tenant_id, reservation_id, layer_kind),
    FOREIGN KEY (tenant_id, reservation_id)
        REFERENCES erpx_agent_write.agent_budget_reservations (tenant_id, reservation_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, layer_kind, subject_ref)
        REFERENCES erpx_agent_write.agent_budget_limits (tenant_id, layer_kind, subject_ref)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_agent_write.agent_assignments (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    assignment_id UUID NOT NULL,
    root_assignment_id UUID NOT NULL,
    parent_assignment_id UUID,
    scope_key TEXT NOT NULL,
    tenant_scope JSONB NOT NULL,
    root_action_ref TEXT NOT NULL,
    root_initiating_principal_ref TEXT NOT NULL,
    root_idempotency_key TEXT NOT NULL,
    trigger_kind TEXT NOT NULL CHECK (trigger_kind IN (
        'mention', 'assign', 'run', 'compare', 'delegation'
    )),
    requested_agent_ref TEXT NOT NULL,
    agent_definition_id UUID NOT NULL,
    agent_version_id UUID NOT NULL,
    provider_ref TEXT NOT NULL,
    model_ref TEXT NOT NULL,
    prompt_digest TEXT NOT NULL CHECK (prompt_digest ~ '^[0-9A-Fa-f]{64}$'),
    policy_digest TEXT NOT NULL CHECK (policy_digest ~ '^[0-9A-Fa-f]{64}$'),
    encrypted_prompt_ref TEXT NOT NULL,
    payload_manifest JSONB NOT NULL,
    assignment_status TEXT NOT NULL CHECK (assignment_status IN (
        'queued', 'running', 'waiting', 'completed', 'failed', 'cancelled'
    )),
    depth INTEGER NOT NULL CHECK (depth >= 0),
    hops INTEGER NOT NULL CHECK (hops >= 0),
    attempt INTEGER NOT NULL CHECK (attempt >= 0),
    budget_reservation_id UUID NOT NULL,
    queue_priority INTEGER NOT NULL CHECK (queue_priority BETWEEN 0 AND 65535),
    not_before TIMESTAMPTZ,
    lease_token UUID,
    lease_owner TEXT,
    lease_acquired_at TIMESTAMPTZ,
    lease_expires_at TIMESTAMPTZ,
    output_digest TEXT CHECK (output_digest IS NULL OR output_digest ~ '^[0-9A-Fa-f]{64}$'),
    encrypted_output_ref TEXT,
    failure_code TEXT,
    failure_retryable BOOLEAN,
    failure_detail_digest TEXT CHECK (
        failure_detail_digest IS NULL OR failure_detail_digest ~ '^[0-9A-Fa-f]{64}$'
    ),
    failure_failed_at TIMESTAMPTZ,
    cancellation_reason TEXT,
    actual_units BIGINT CHECK (actual_units >= 0),
    actual_cost_micros BIGINT CHECK (actual_cost_micros >= 0),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, assignment_id),
    UNIQUE (tenant_id, budget_reservation_id),
    UNIQUE (tenant_id, scope_key, root_action_ref, requested_agent_ref),
    FOREIGN KEY (tenant_id, agent_definition_id) REFERENCES
        erpx_agent_write.agent_definitions (tenant_id, agent_definition_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, agent_version_id) REFERENCES
        erpx_agent_write.agent_versions (tenant_id, agent_version_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, budget_reservation_id) REFERENCES
        erpx_agent_write.agent_budget_reservations (tenant_id, reservation_id) ON DELETE RESTRICT,
    CHECK ((parent_assignment_id IS NULL) = (assignment_id = root_assignment_id)),
    CHECK ((lease_token IS NULL) = (lease_owner IS NULL)),
    CHECK ((lease_token IS NULL) = (lease_acquired_at IS NULL)),
    CHECK ((lease_token IS NULL) = (lease_expires_at IS NULL)),
    CHECK ((assignment_status = 'running') = (lease_token IS NOT NULL)),
    CHECK ((failure_code IS NULL) = (failure_retryable IS NULL)),
    CHECK ((failure_code IS NULL) = (failure_detail_digest IS NULL)),
    CHECK ((failure_code IS NULL) = (failure_failed_at IS NULL)),
    CHECK (cancellation_reason IS NULL OR assignment_status = 'cancelled'),
    CHECK (updated_at >= created_at)
);

ALTER TABLE erpx_agent_write.agent_budget_reservations
    ADD CONSTRAINT agent_budget_assignment_fk FOREIGN KEY (tenant_id, assignment_id)
        REFERENCES erpx_agent_write.agent_assignments (tenant_id, assignment_id)
        DEFERRABLE INITIALLY DEFERRED;

ALTER TABLE erpx_agent_write.agent_assignments
    ADD CONSTRAINT agent_assignment_root_fk FOREIGN KEY (tenant_id, root_assignment_id)
        REFERENCES erpx_agent_write.agent_assignments (tenant_id, assignment_id)
        DEFERRABLE INITIALLY DEFERRED,
    ADD CONSTRAINT agent_assignment_parent_fk FOREIGN KEY (tenant_id, parent_assignment_id)
        REFERENCES erpx_agent_write.agent_assignments (tenant_id, assignment_id)
        ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED;

CREATE INDEX agent_assignment_due_queue
    ON erpx_agent_write.agent_assignments
        (tenant_id, queue_priority DESC, not_before, created_at, assignment_id)
    WHERE assignment_status = 'queued';

CREATE INDEX agent_assignment_expired_leases
    ON erpx_agent_write.agent_assignments (tenant_id, lease_expires_at, assignment_id)
    WHERE assignment_status = 'running';

CREATE TABLE erpx_agent_write.assignment_delegations (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    child_assignment_id UUID NOT NULL,
    root_assignment_id UUID NOT NULL,
    parent_assignment_id UUID NOT NULL,
    parent_agent_definition_id UUID NOT NULL,
    child_agent_definition_id UUID NOT NULL,
    reason_digest TEXT NOT NULL CHECK (reason_digest ~ '^[0-9A-Fa-f]{64}$'),
    scope_key TEXT NOT NULL,
    depth INTEGER NOT NULL CHECK (depth > 0),
    hops INTEGER NOT NULL CHECK (hops > 0),
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, child_assignment_id),
    FOREIGN KEY (tenant_id, child_assignment_id) REFERENCES
        erpx_agent_write.agent_assignments (tenant_id, assignment_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, root_assignment_id) REFERENCES
        erpx_agent_write.agent_assignments (tenant_id, assignment_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, parent_assignment_id) REFERENCES
        erpx_agent_write.agent_assignments (tenant_id, assignment_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, parent_agent_definition_id) REFERENCES
        erpx_agent_write.agent_definitions (tenant_id, agent_definition_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, child_agent_definition_id) REFERENCES
        erpx_agent_write.agent_definitions (tenant_id, agent_definition_id) ON DELETE RESTRICT,
    CHECK (parent_agent_definition_id <> child_agent_definition_id)
);

CREATE INDEX agent_delegation_parent
    ON erpx_agent_write.assignment_delegations
        (tenant_id, root_assignment_id, parent_assignment_id, child_assignment_id);

CREATE TABLE erpx_agent_write.assignment_checkpoints (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    checkpoint_id UUID NOT NULL,
    assignment_id UUID NOT NULL,
    step INTEGER NOT NULL CHECK (step > 0),
    state_digest TEXT NOT NULL CHECK (state_digest ~ '^[0-9A-Fa-f]{64}$'),
    retry_safe BOOLEAN NOT NULL,
    lease_token UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, checkpoint_id),
    UNIQUE (tenant_id, assignment_id, step),
    FOREIGN KEY (tenant_id, assignment_id) REFERENCES
        erpx_agent_write.agent_assignments (tenant_id, assignment_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_agent_write.agent_pools (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    agent_pool_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    tenant_scope JSONB NOT NULL,
    pool_name TEXT NOT NULL,
    pool_version BIGINT NOT NULL CHECK (pool_version > 0),
    enabled BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, agent_pool_id),
    UNIQUE (tenant_id, scope_key, pool_name),
    CHECK (updated_at >= created_at)
);

CREATE TABLE erpx_agent_write.agent_pool_members (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    agent_pool_id UUID NOT NULL,
    agent_version_id UUID NOT NULL,
    PRIMARY KEY (tenant_id, agent_pool_id, agent_version_id),
    FOREIGN KEY (tenant_id, agent_pool_id) REFERENCES
        erpx_agent_write.agent_pools (tenant_id, agent_pool_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, agent_version_id) REFERENCES
        erpx_agent_write.agent_versions (tenant_id, agent_version_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_agent_write.agent_pool_routing_decisions (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    routing_decision_id UUID NOT NULL,
    agent_pool_id UUID NOT NULL,
    pool_version BIGINT NOT NULL CHECK (pool_version > 0),
    root_action_ref TEXT NOT NULL,
    capability TEXT NOT NULL,
    requested_scope JSONB NOT NULL,
    candidate_version_ids UUID[] NOT NULL,
    selected_agent_version_id UUID NOT NULL,
    selected_agent_version_ids UUID[] NOT NULL,
    decision_digest TEXT NOT NULL CHECK (decision_digest ~ '^[0-9A-Fa-f]{64}$'),
    compare_requested BOOLEAN NOT NULL,
    previewed_cost_micros BIGINT CHECK (previewed_cost_micros >= 0),
    initiating_human_ref TEXT,
    decided_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, routing_decision_id),
    UNIQUE (tenant_id, agent_pool_id, root_action_ref),
    FOREIGN KEY (tenant_id, agent_pool_id) REFERENCES
        erpx_agent_write.agent_pools (tenant_id, agent_pool_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, selected_agent_version_id) REFERENCES
        erpx_agent_write.agent_versions (tenant_id, agent_version_id) ON DELETE RESTRICT,
    CHECK (cardinality(candidate_version_ids) > 0),
    CHECK (cardinality(selected_agent_version_ids) > 0),
    CHECK (
        (compare_requested AND previewed_cost_micros > 0 AND initiating_human_ref IS NOT NULL
            AND cardinality(selected_agent_version_ids) > 1)
        OR (NOT compare_requested AND previewed_cost_micros IS NULL
            AND initiating_human_ref IS NULL AND cardinality(selected_agent_version_ids) = 1)
    )
);

CREATE TABLE erpx_agent_write.agent_sessions (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    agent_session_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    tenant_scope JSONB NOT NULL,
    owner_human_ref TEXT NOT NULL,
    record_id UUID,
    descriptor_version BIGINT CHECK (descriptor_version > 0),
    source_event_position BIGINT CHECK (source_event_position > 0),
    expires_at TIMESTAMPTZ NOT NULL,
    legal_hold BOOLEAN NOT NULL DEFAULT FALSE,
    expired_at TIMESTAMPTZ,
    session_version BIGINT NOT NULL CHECK (session_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, agent_session_id),
    CHECK (expires_at > created_at),
    CHECK (updated_at >= created_at),
    CHECK ((record_id IS NULL) = (descriptor_version IS NULL)),
    CHECK ((record_id IS NULL) = (source_event_position IS NULL))
);

CREATE INDEX agent_private_session_expiry
    ON erpx_agent_write.agent_sessions (tenant_id, expires_at, agent_session_id)
    WHERE expired_at IS NULL AND legal_hold = FALSE;

CREATE TABLE erpx_agent_write.agent_session_messages (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    session_message_id UUID NOT NULL,
    agent_session_id UUID NOT NULL,
    author_kind TEXT NOT NULL CHECK (author_kind IN ('human', 'agent')),
    author_ref TEXT NOT NULL,
    encrypted_payload_ref TEXT NOT NULL,
    content_digest TEXT NOT NULL CHECK (content_digest ~ '^[0-9A-Fa-f]{64}$'),
    message_version BIGINT NOT NULL CHECK (message_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    published_at TIMESTAMPTZ,
    publishing_approver_human_ref TEXT,
    publication_anchor_ref UUID,
    PRIMARY KEY (tenant_id, session_message_id),
    FOREIGN KEY (tenant_id, agent_session_id) REFERENCES
        erpx_agent_write.agent_sessions (tenant_id, agent_session_id) ON DELETE RESTRICT,
    CHECK (
        published_at IS NULL
        OR (author_kind = 'agent' AND publishing_approver_human_ref IS NOT NULL
            AND publication_anchor_ref IS NOT NULL)
    )
);

CREATE TABLE erpx_agent_write.agent_action_cards (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    action_card_id UUID NOT NULL,
    assignment_id UUID NOT NULL,
    agent_principal_ref TEXT NOT NULL,
    owner_context TEXT NOT NULL,
    command_type TEXT NOT NULL,
    payload_digest TEXT NOT NULL CHECK (payload_digest ~ '^[0-9A-Fa-f]{64}$'),
    action_status TEXT NOT NULL CHECK (action_status IN (
        'proposed', 'rejected', 'confirmed_for_handoff'
    )),
    confirmed_by_human_ref TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    resolved_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, action_card_id),
    FOREIGN KEY (tenant_id, assignment_id) REFERENCES
        erpx_agent_write.agent_assignments (tenant_id, assignment_id) ON DELETE RESTRICT,
    CHECK (
        (action_status = 'confirmed_for_handoff' AND confirmed_by_human_ref IS NOT NULL)
        OR action_status <> 'confirmed_for_handoff'
    )
);

CREATE TABLE erpx_agent_write.provider_invocation_evidence (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    invocation_id UUID NOT NULL,
    assignment_id UUID NOT NULL,
    assignment_version BIGINT NOT NULL CHECK (assignment_version > 0),
    attempt INTEGER NOT NULL CHECK (attempt > 0),
    agent_definition_id UUID NOT NULL,
    agent_version_id UUID NOT NULL,
    provider_ref TEXT NOT NULL,
    model_ref TEXT NOT NULL,
    policy_version BIGINT NOT NULL CHECK (policy_version > 0),
    payload_digest TEXT NOT NULL CHECK (payload_digest ~ '^[0-9A-Fa-f]{64}$'),
    policy_digest TEXT NOT NULL CHECK (policy_digest ~ '^[0-9A-Fa-f]{64}$'),
    field_refs TEXT[] NOT NULL,
    document_refs UUID[] NOT NULL,
    classification TEXT NOT NULL CHECK (classification IN (
        'public', 'internal', 'confidential', 'restricted'
    )),
    residency TEXT NOT NULL,
    output_digest TEXT CHECK (output_digest ~ '^[0-9A-Fa-f]{64}$'),
    provider_units BIGINT CHECK (provider_units > 0),
    provider_cost_micros BIGINT CHECK (provider_cost_micros >= 0),
    evidence_version BIGINT NOT NULL CHECK (evidence_version > 0),
    invoked_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    outcome TEXT CHECK (outcome IN ('completed', 'failed', 'cancelled')),
    PRIMARY KEY (tenant_id, invocation_id),
    UNIQUE (tenant_id, assignment_id, attempt),
    FOREIGN KEY (tenant_id, assignment_id) REFERENCES
        erpx_agent_write.agent_assignments (tenant_id, assignment_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, agent_definition_id) REFERENCES
        erpx_agent_write.agent_definitions (tenant_id, agent_definition_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, agent_version_id) REFERENCES
        erpx_agent_write.agent_versions (tenant_id, agent_version_id) ON DELETE RESTRICT,
    CHECK (
        (outcome = 'completed' AND completed_at IS NOT NULL AND output_digest IS NOT NULL
            AND provider_units IS NOT NULL AND provider_cost_micros IS NOT NULL
            AND evidence_version = 2)
        OR (outcome IN ('failed', 'cancelled') AND completed_at IS NOT NULL
            AND output_digest IS NULL AND provider_units IS NULL
            AND provider_cost_micros IS NULL AND evidence_version = 2)
        OR (outcome IS NULL AND completed_at IS NULL AND output_digest IS NULL
            AND provider_units IS NULL AND provider_cost_micros IS NULL
            AND evidence_version = 1)
    )
);

CREATE TABLE erpx_agent_write.agent_governance_records (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    record_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    tenant_scope JSONB NOT NULL,
    record_kind TEXT NOT NULL CHECK (record_kind IN ('delegation_tree','budget_decision')),
    subject_record_id UUID NOT NULL,
    policy_digest TEXT NOT NULL CHECK (policy_digest ~ '^[0-9A-Fa-f]{64}$'),
    lifecycle_state TEXT NOT NULL CHECK (lifecycle_state IN ('proposed','approved','active','completed','denied','expired','cancelled')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,record_id),
    UNIQUE (tenant_id,record_kind,subject_record_id,aggregate_version)
);

CREATE TABLE erpx_agent_write.agent_command_receipts (
    tenant_id UUID NOT NULL DEFAULT erpx_agent_write.current_tenant_id(),
    scope_key TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    request_digest TEXT NOT NULL CHECK (request_digest ~ '^[0-9A-Fa-f]{64}$'),
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN (
        'agent_definition', 'assignment', 'agent_pool', 'pool_routing',
        'agent_session', 'provider_invocation', 'delegation_tree', 'budget_decision'
    )),
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, scope_key, idempotency_key)
);

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'agent_definitions','agent_versions','agent_version_capabilities',
        'agent_version_tools','agent_version_knowledge_scopes',
        'agent_version_organization_scopes','agent_version_evaluations',
        'agent_provider_policies','agent_provider_policy_models',
        'agent_provider_policy_use_cases','agent_provider_policy_residencies',
        'agent_provider_policy_evaluations','agent_provider_policy_revocations',
        'agent_material_retention_policies','agent_budget_limits',
        'agent_budget_reservations','agent_budget_reservation_layers',
        'agent_assignments','assignment_delegations',
        'assignment_checkpoints','agent_pools','agent_pool_members',
        'agent_pool_routing_decisions','agent_sessions','agent_session_messages',
        'agent_action_cards','provider_invocation_evidence','agent_governance_records','agent_command_receipts'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_agent_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_agent_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY agent_tenant_isolation ON erpx_agent_write.%I '
            'USING (tenant_id = erpx_agent_write.current_tenant_id()) '
            'WITH CHECK (tenant_id = erpx_agent_write.current_tenant_id())',
            table_name
        );
    END LOOP;
END;
$$;
