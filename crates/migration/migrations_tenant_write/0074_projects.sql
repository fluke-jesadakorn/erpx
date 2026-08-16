-- Normalized Projects write authority. JSON columns below are bounded value
-- objects (tenant scope or typed references), never aggregate snapshots.
CREATE SCHEMA IF NOT EXISTS erpx_projects_write;

CREATE OR REPLACE FUNCTION erpx_projects_write.current_tenant_id()
RETURNS UUID LANGUAGE SQL STABLE AS $$
    SELECT nullif(current_setting('app.tenant_id', true), '')::uuid
$$;

CREATE TABLE erpx_projects_write.project_guards (
    tenant_id UUID NOT NULL DEFAULT erpx_projects_write.current_tenant_id(),
    project_id UUID NOT NULL, tenant_scope JSONB NOT NULL, scope_key TEXT NOT NULL,
    project_code TEXT NOT NULL CHECK (octet_length(project_code) BETWEEN 1 AND 128),
    project_name TEXT NOT NULL CHECK (octet_length(project_name) BETWEEN 1 AND 255),
    project_class TEXT NOT NULL CHECK (project_class IN ('client_services','internal','capital')),
    project_status TEXT NOT NULL CHECK (project_status IN ('proposed','approved','active','paused','completed','closed','cancelled','archived')),
    policy_ref TEXT NOT NULL CHECK (octet_length(policy_ref) BETWEEN 1 AND 255),
    workflow_template_id UUID NOT NULL, workflow_version BIGINT NOT NULL CHECK (workflow_version > 0),
    owner_id UUID NOT NULL, aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL, updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, project_id), UNIQUE (tenant_id, scope_key, project_code),
    CHECK (updated_at >= created_at)
);

CREATE TABLE erpx_projects_write.project_node_guards (
    tenant_id UUID NOT NULL DEFAULT erpx_projects_write.current_tenant_id(),
    project_id UUID NOT NULL, node_id UUID NOT NULL, parent_node_id UUID,
    node_kind TEXT NOT NULL CHECK (node_kind IN ('phase','epic','task','milestone','deliverable','risk','issue','decision','change_request')),
    title TEXT NOT NULL CHECK (octet_length(title) BETWEEN 1 AND 255), owner_id UUID NOT NULL,
    planned_start DATE, planned_end DATE, actual_start DATE, actual_end DATE,
    workflow_state TEXT NOT NULL CHECK (octet_length(workflow_state) BETWEEN 1 AND 64),
    core_state TEXT NOT NULL CHECK (core_state IN ('proposed','open','in_progress','waiting','resolved','closed','cancelled')),
    promotion_thread_id UUID, promotion_comment_id UUID, promotion_revision BIGINT CHECK (promotion_revision > 0),
    promotion_digest TEXT CHECK (promotion_digest ~ '^(sha256:)?[0-9A-Fa-f]{64}$'),
    node_version BIGINT NOT NULL CHECK (node_version > 0), created_at TIMESTAMPTZ NOT NULL, updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, project_id, node_id),
    FOREIGN KEY (tenant_id, project_id) REFERENCES erpx_projects_write.project_guards(tenant_id, project_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, project_id, parent_node_id) REFERENCES erpx_projects_write.project_node_guards(tenant_id, project_id, node_id) DEFERRABLE INITIALLY DEFERRED,
    CHECK (parent_node_id IS NULL OR parent_node_id <> node_id),
    CHECK (planned_end IS NULL OR planned_start IS NULL OR planned_end >= planned_start),
    CHECK (actual_end IS NULL OR actual_start IS NULL OR actual_end >= actual_start),
    CHECK ((promotion_thread_id IS NULL) = (promotion_comment_id IS NULL)),
    CHECK ((promotion_thread_id IS NULL) = (promotion_revision IS NULL)),
    CHECK ((promotion_thread_id IS NULL) = (promotion_digest IS NULL)), CHECK (updated_at >= created_at)
);

CREATE TABLE erpx_projects_write.project_node_assignees (
    tenant_id UUID NOT NULL DEFAULT erpx_projects_write.current_tenant_id(), project_id UUID NOT NULL,
    node_id UUID NOT NULL, assignee_id UUID NOT NULL, PRIMARY KEY (tenant_id, project_id, node_id, assignee_id),
    FOREIGN KEY (tenant_id, project_id, node_id) REFERENCES erpx_projects_write.project_node_guards(tenant_id, project_id, node_id) ON DELETE CASCADE
);
CREATE TABLE erpx_projects_write.project_node_attachments (
    tenant_id UUID NOT NULL DEFAULT erpx_projects_write.current_tenant_id(), project_id UUID NOT NULL,
    node_id UUID NOT NULL, attachment_id UUID NOT NULL, object_ref TEXT NOT NULL,
    content_digest TEXT NOT NULL CHECK (content_digest ~ '^(sha256:)?[0-9A-Fa-f]{64}$'),
    PRIMARY KEY (tenant_id, project_id, node_id, attachment_id),
    FOREIGN KEY (tenant_id, project_id, node_id) REFERENCES erpx_projects_write.project_node_guards(tenant_id, project_id, node_id) ON DELETE CASCADE
);
CREATE TABLE erpx_projects_write.project_node_references (
    tenant_id UUID NOT NULL DEFAULT erpx_projects_write.current_tenant_id(), project_id UUID NOT NULL,
    node_id UUID NOT NULL, record_id UUID NOT NULL, owner_context TEXT NOT NULL, record_type TEXT NOT NULL,
    reference_document JSONB NOT NULL, PRIMARY KEY (tenant_id, project_id, node_id, record_id, owner_context, record_type),
    FOREIGN KEY (tenant_id, project_id, node_id) REFERENCES erpx_projects_write.project_node_guards(tenant_id, project_id, node_id) ON DELETE CASCADE
);
CREATE TABLE erpx_projects_write.project_time_facts (
    tenant_id UUID NOT NULL DEFAULT erpx_projects_write.current_tenant_id(), project_id UUID NOT NULL,
    node_id UUID NOT NULL, fact_id UUID NOT NULL, worker_id UUID NOT NULL, work_date DATE NOT NULL,
    minutes INTEGER NOT NULL CHECK (minutes BETWEEN 1 AND 1440), source_record JSONB NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0), PRIMARY KEY (tenant_id, project_id, node_id, fact_id),
    FOREIGN KEY (tenant_id, project_id, node_id) REFERENCES erpx_projects_write.project_node_guards(tenant_id, project_id, node_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_projects_write.project_cost_facts (
    tenant_id UUID NOT NULL DEFAULT erpx_projects_write.current_tenant_id(), project_id UUID NOT NULL,
    node_id UUID NOT NULL, fact_id UUID NOT NULL, amount_minor BIGINT NOT NULL CHECK (amount_minor <> 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'), source_record JSONB NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0), reversed_fact_id UUID,
    PRIMARY KEY (tenant_id, project_id, node_id, fact_id),
    FOREIGN KEY (tenant_id, project_id, node_id) REFERENCES erpx_projects_write.project_node_guards(tenant_id, project_id, node_id) ON DELETE RESTRICT
);

CREATE OR REPLACE FUNCTION erpx_projects_write.reject_project_node_cycle()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE cycle_found BOOLEAN;
BEGIN
    IF NEW.parent_node_id IS NULL THEN RETURN NEW; END IF;
    WITH RECURSIVE ancestors(node_id) AS (
        SELECT NEW.parent_node_id UNION ALL SELECT parent.parent_node_id
        FROM erpx_projects_write.project_node_guards parent JOIN ancestors current
          ON parent.tenant_id=NEW.tenant_id AND parent.project_id=NEW.project_id AND parent.node_id=current.node_id
        WHERE parent.parent_node_id IS NOT NULL
    ) SELECT EXISTS (SELECT 1 FROM ancestors WHERE node_id=NEW.node_id) INTO cycle_found;
    IF cycle_found THEN RAISE EXCEPTION 'project hierarchy cycle'; END IF; RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER projects_node_cycle_guard AFTER INSERT OR UPDATE OF parent_node_id
ON erpx_projects_write.project_node_guards DEFERRABLE INITIALLY DEFERRED FOR EACH ROW
EXECUTE FUNCTION erpx_projects_write.reject_project_node_cycle();

CREATE TABLE erpx_projects_write.workflow_template_versions (
    tenant_id UUID NOT NULL DEFAULT erpx_projects_write.current_tenant_id(), template_id UUID NOT NULL,
    template_version BIGINT NOT NULL CHECK (template_version > 0), tenant_scope JSONB NOT NULL, scope_key TEXT NOT NULL,
    template_name TEXT NOT NULL CHECK (octet_length(template_name) BETWEEN 1 AND 255),
    template_status TEXT NOT NULL CHECK (template_status IN ('active','withdrawn')),
    created_at TIMESTAMPTZ NOT NULL, published_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, template_id, template_version), CHECK (published_at >= created_at)
);
CREATE TABLE erpx_projects_write.workflow_states (
    tenant_id UUID NOT NULL DEFAULT erpx_projects_write.current_tenant_id(), template_id UUID NOT NULL,
    template_version BIGINT NOT NULL, state_code TEXT NOT NULL CHECK (octet_length(state_code) BETWEEN 1 AND 64),
    core_state TEXT NOT NULL CHECK (core_state IN ('proposed','open','in_progress','waiting','resolved','closed','cancelled')),
    PRIMARY KEY (tenant_id, template_id, template_version, state_code),
    FOREIGN KEY (tenant_id, template_id, template_version) REFERENCES erpx_projects_write.workflow_template_versions(tenant_id, template_id, template_version) ON DELETE CASCADE
);
CREATE TABLE erpx_projects_write.workflow_transitions (
    tenant_id UUID NOT NULL DEFAULT erpx_projects_write.current_tenant_id(), template_id UUID NOT NULL,
    template_version BIGINT NOT NULL, from_state TEXT NOT NULL, to_state TEXT NOT NULL,
    PRIMARY KEY (tenant_id, template_id, template_version, from_state, to_state),
    FOREIGN KEY (tenant_id, template_id, template_version, from_state) REFERENCES erpx_projects_write.workflow_states(tenant_id, template_id, template_version, state_code) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, template_id, template_version, to_state) REFERENCES erpx_projects_write.workflow_states(tenant_id, template_id, template_version, state_code) ON DELETE CASCADE,
    CHECK (from_state <> to_state)
);

CREATE TABLE erpx_projects_write.dependency_guards (
    tenant_id UUID NOT NULL DEFAULT erpx_projects_write.current_tenant_id(), dependency_id UUID NOT NULL,
    terms_version BIGINT NOT NULL CHECK (terms_version > 0), source_project_id UUID NOT NULL,
    source_project_version BIGINT NOT NULL CHECK (source_project_version > 0), source_node_id UUID NOT NULL,
    source_node_version BIGINT NOT NULL CHECK (source_node_version > 0), source_scope JSONB NOT NULL, source_owner_id UUID NOT NULL,
    target_project_id UUID NOT NULL, target_project_version BIGINT NOT NULL CHECK (target_project_version > 0), target_node_id UUID NOT NULL,
    target_node_version BIGINT NOT NULL CHECK (target_node_version > 0), target_scope JSONB NOT NULL, target_owner_id UUID NOT NULL,
    dependency_kind TEXT NOT NULL CHECK (dependency_kind IN ('finish_to_start','start_to_start','finish_to_finish')),
    dependency_status TEXT NOT NULL CHECK (dependency_status IN ('proposed','accepted','rejected','revoked','cancelled')),
    proposed_by UUID NOT NULL, aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL, updated_at TIMESTAMPTZ NOT NULL,
    CHECK (source_project_id <> target_project_id), CHECK (proposed_by = source_owner_id), CHECK (updated_at >= created_at),
    PRIMARY KEY (tenant_id, dependency_id),
    UNIQUE (tenant_id, source_project_id, source_node_id, target_project_id, target_node_id, dependency_kind)
);
CREATE TABLE erpx_projects_write.dependency_acceptances (
    tenant_id UUID NOT NULL DEFAULT erpx_projects_write.current_tenant_id(), dependency_id UUID NOT NULL,
    dependency_side TEXT NOT NULL CHECK (dependency_side IN ('source','target')), owner_id UUID NOT NULL,
    terms_version BIGINT NOT NULL CHECK (terms_version > 0), accepted_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, dependency_id, dependency_side),
    FOREIGN KEY (tenant_id, dependency_id) REFERENCES erpx_projects_write.dependency_guards(tenant_id, dependency_id) ON DELETE CASCADE
);

CREATE TABLE erpx_projects_write.handoff_guards (
    tenant_id UUID NOT NULL DEFAULT erpx_projects_write.current_tenant_id(), handoff_id UUID NOT NULL,
    project_id UUID NOT NULL, item_id UUID NOT NULL, tenant_scope JSONB NOT NULL, scope_key TEXT NOT NULL,
    handoff_kind TEXT NOT NULL, owner_context TEXT NOT NULL CHECK (owner_context IN ('finance','legal','operations','assets','sales','procurement')),
    handoff_status TEXT NOT NULL CHECK (handoff_status IN ('proposed','approved','ready','accepted','rejected','uncertain','cancelled')),
    idempotency_key TEXT NOT NULL CHECK (octet_length(idempotency_key) BETWEEN 1 AND 255), correlation_id UUID NOT NULL,
    payload_digest TEXT NOT NULL CHECK (payload_digest ~ '^(sha256:)?[0-9A-Fa-f]{64}$'), proposed_by UUID NOT NULL,
    approved_by UUID, outcome_owner_context TEXT, outcome_receipt_id TEXT, outcome_record JSONB,
    outcome_accepted BOOLEAN, outcome_reason_code TEXT, reconciled_at TIMESTAMPTZ,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), created_at TIMESTAMPTZ NOT NULL, updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, handoff_id), UNIQUE (tenant_id, scope_key, owner_context, idempotency_key),
    CHECK (updated_at >= created_at), CHECK (approved_by IS NULL OR approved_by <> proposed_by),
    CHECK ((outcome_receipt_id IS NULL) = (outcome_accepted IS NULL)),
    CHECK ((outcome_receipt_id IS NULL) = (outcome_reason_code IS NULL)),
    CHECK ((outcome_receipt_id IS NULL) = (reconciled_at IS NULL))
);

CREATE INDEX projects_node_parent ON erpx_projects_write.project_node_guards(tenant_id, project_id, parent_node_id, node_id);
CREATE INDEX projects_dependency_active ON erpx_projects_write.dependency_guards(tenant_id, source_project_id, target_project_id) WHERE dependency_status='accepted';
CREATE INDEX projects_handoff_reconciliation ON erpx_projects_write.handoff_guards(tenant_id, scope_key, handoff_status, updated_at) WHERE handoff_status IN ('ready','uncertain');

DO $$ DECLARE table_name TEXT; BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'project_guards','project_node_guards','project_node_assignees','project_node_attachments',
        'project_node_references','project_time_facts','project_cost_facts','workflow_template_versions',
        'workflow_states','workflow_transitions','dependency_guards','dependency_acceptances','handoff_guards'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_projects_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_projects_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format('CREATE POLICY projects_tenant_isolation ON erpx_projects_write.%I USING (tenant_id=erpx_projects_write.current_tenant_id()) WITH CHECK (tenant_id=erpx_projects_write.current_tenant_id())', table_name);
    END LOOP;
END; $$;
