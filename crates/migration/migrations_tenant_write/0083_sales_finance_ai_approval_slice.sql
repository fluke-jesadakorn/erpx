-- Durable authority for the Sales-to-Finance vertical slice. Each bounded
-- context owns its rows; cross-context links are immutable identifiers and
-- versions delivered through the shared CQRS outbox.

CREATE TABLE erpx_agent_write.suggestion_envelopes (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    suggestion_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    owner_context TEXT NOT NULL CHECK (owner_context IN ('sales', 'finance')),
    capability TEXT NOT NULL CHECK (capability IN (
        'sales.quote.compose.v1', 'sales.revenue-arrangement.v1',
        'finance.accounting-plan.v1', 'finance.posting-review.v1'
    )),
    source_record_kind TEXT NOT NULL,
    source_record_id UUID NOT NULL,
    source_record_version BIGINT NOT NULL CHECK (source_record_version > 0),
    schema_version INTEGER NOT NULL CHECK (schema_version > 0),
    policy_version BIGINT NOT NULL CHECK (policy_version > 0),
    payload_digest TEXT NOT NULL CHECK (payload_digest ~ '^[0-9A-Fa-f]{64}$'),
    output_digest TEXT CHECK (output_digest ~ '^[0-9A-Fa-f]{64}$'),
    provider_kind TEXT NOT NULL CHECK (provider_kind IN ('fixture', 'ollama', 'open_ai_compatible')),
    provider_ref TEXT NOT NULL,
    model_ref TEXT NOT NULL,
    prompt_digest TEXT NOT NULL CHECK (prompt_digest ~ '^[0-9A-Fa-f]{64}$'),
    invocation_evidence_ref TEXT NOT NULL,
    suggestion_status TEXT NOT NULL CHECK (suggestion_status IN (
        'queued', 'running', 'ready', 'applied', 'rejected', 'superseded', 'failed'
    )),
    requested_by TEXT NOT NULL,
    applied_by TEXT,
    failure_code TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, suggestion_id),
    CHECK (updated_at >= created_at),
    CHECK (suggestion_status NOT IN ('ready', 'applied', 'rejected') OR output_digest IS NOT NULL),
    CHECK (suggestion_status NOT IN ('queued', 'running', 'failed') OR output_digest IS NULL)
);

CREATE INDEX agent_suggestion_source_version
    ON erpx_agent_write.suggestion_envelopes
        (tenant_id, owner_context, source_record_id, source_record_version, updated_at DESC);

CREATE TABLE erpx_operations_write.approval_sessions (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    approval_session_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    subject_context TEXT NOT NULL CHECK (subject_context IN ('sales', 'finance')),
    subject_kind TEXT NOT NULL,
    subject_id UUID NOT NULL,
    subject_version BIGINT NOT NULL CHECK (subject_version > 0),
    policy_version BIGINT NOT NULL CHECK (policy_version > 0),
    requested_by TEXT NOT NULL,
    session_status TEXT NOT NULL CHECK (session_status IN (
        'pending', 'approved', 'rejected', 'superseded', 'cancelled'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, approval_session_id),
    UNIQUE (tenant_id, subject_context, subject_kind, subject_id, subject_version, policy_version),
    CHECK (updated_at >= created_at)
);

CREATE TABLE erpx_operations_write.approval_session_gates (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    approval_session_id UUID NOT NULL,
    gate_kind TEXT NOT NULL CHECK (gate_kind IN (
        'sales_commercial', 'finance_accounting_plan', 'finance_posting'
    )),
    assigned_to TEXT,
    gate_status TEXT NOT NULL CHECK (gate_status IN ('pending', 'approved', 'rejected', 'cancelled')),
    decided_by TEXT,
    reason_code TEXT,
    decided_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, approval_session_id, gate_kind),
    FOREIGN KEY (tenant_id, approval_session_id) REFERENCES
        erpx_operations_write.approval_sessions (tenant_id, approval_session_id) ON DELETE RESTRICT,
    CHECK (
        (gate_status = 'pending' AND decided_by IS NULL AND reason_code IS NULL AND decided_at IS NULL)
        OR (gate_status IN ('approved', 'rejected') AND decided_by IS NOT NULL AND reason_code IS NOT NULL AND decided_at IS NOT NULL)
        OR gate_status = 'cancelled'
    )
);

CREATE INDEX operations_my_approval_work
    ON erpx_operations_write.approval_session_gates
        (tenant_id, assigned_to, gate_status, approval_session_id)
    WHERE gate_status = 'pending';

CREATE TABLE erpx_sales_write.quote_workflows (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    quote_id UUID NOT NULL,
    workflow_version BIGINT NOT NULL CHECK (workflow_version > 0),
    quote_version BIGINT NOT NULL CHECK (quote_version > 0),
    customer_id UUID NOT NULL,
    customer_version BIGINT NOT NULL CHECK (customer_version > 0),
    quote_code TEXT NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    payment_terms_days INTEGER NOT NULL CHECK (payment_terms_days BETWEEN 0 AND 365),
    valid_until TIMESTAMPTZ NOT NULL,
    document_language TEXT NOT NULL CHECK (document_language IN ('thai', 'english')),
    notes TEXT NOT NULL,
    maker TEXT NOT NULL,
    workflow_status TEXT NOT NULL CHECK (workflow_status IN (
        'draft', 'pending_approval', 'approval_rejected', 'approved', 'pdf_generated',
        'customer_accepted', 'customer_rejected', 'superseded'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, quote_id),
    UNIQUE (tenant_id, quote_code),
    FOREIGN KEY (quote_id) REFERENCES erpx_sales_write.quote_guards (quote_id) ON DELETE RESTRICT,
    CHECK (updated_at >= created_at)
);

CREATE TABLE erpx_sales_write.quote_workflow_lines (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    quote_id UUID NOT NULL,
    quote_version BIGINT NOT NULL CHECK (quote_version > 0),
    line_id UUID NOT NULL,
    line_source TEXT NOT NULL CHECK (line_source IN ('catalog', 'custom')),
    item_id UUID,
    item_version BIGINT CHECK (item_version > 0),
    custom_reason TEXT,
    item_code TEXT NOT NULL,
    description TEXT NOT NULL,
    quantity BIGINT NOT NULL CHECK (quantity > 0),
    unit_price_minor BIGINT NOT NULL CHECK (unit_price_minor >= 0),
    discount_basis_points INTEGER NOT NULL CHECK (discount_basis_points BETWEEN 0 AND 10000),
    tax_profile_id UUID NOT NULL,
    tax_profile_version BIGINT NOT NULL CHECK (tax_profile_version > 0),
    tax_classification TEXT NOT NULL CHECK (tax_classification IN ('standard_vat', 'zero_rated', 'exempt')),
    tax_rate_basis_points INTEGER NOT NULL CHECK (tax_rate_basis_points BETWEEN 0 AND 10000),
    tax_authority_ref TEXT NOT NULL,
    PRIMARY KEY (tenant_id, quote_id, quote_version, line_id),
    FOREIGN KEY (tenant_id, quote_id) REFERENCES
        erpx_sales_write.quote_workflows (tenant_id, quote_id) ON DELETE RESTRICT,
    CHECK (
        (line_source = 'catalog' AND item_id IS NOT NULL AND item_version IS NOT NULL AND custom_reason IS NULL)
        OR (line_source = 'custom' AND item_id IS NULL AND item_version IS NULL AND custom_reason IS NOT NULL)
    )
);

CREATE TABLE erpx_sales_write.quote_applied_suggestions (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    quote_id UUID NOT NULL,
    quote_version BIGINT NOT NULL CHECK (quote_version > 0),
    suggestion_id UUID NOT NULL,
    source_quote_version BIGINT NOT NULL CHECK (source_quote_version > 0),
    payload_digest TEXT NOT NULL CHECK (payload_digest ~ '^[0-9A-Fa-f]{64}$'),
    output_digest TEXT NOT NULL CHECK (output_digest ~ '^[0-9A-Fa-f]{64}$'),
    provider_label TEXT NOT NULL,
    applied_by TEXT NOT NULL,
    applied_at TIMESTAMPTZ NOT NULL,
    superseded_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, quote_id, quote_version),
    FOREIGN KEY (tenant_id, quote_id) REFERENCES
        erpx_sales_write.quote_workflows (tenant_id, quote_id) ON DELETE RESTRICT,
    CHECK (source_quote_version < quote_version),
    CHECK (superseded_at IS NULL OR superseded_at >= applied_at)
);

CREATE TABLE erpx_sales_write.quote_approval_links (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    quote_id UUID NOT NULL,
    quote_version BIGINT NOT NULL CHECK (quote_version > 0),
    approval_session_id UUID NOT NULL,
    policy_version BIGINT NOT NULL CHECK (policy_version > 0),
    sales_approver TEXT NOT NULL,
    finance_approver TEXT NOT NULL,
    approved_at TIMESTAMPTZ NOT NULL,
    superseded_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, quote_id, quote_version),
    FOREIGN KEY (tenant_id, quote_id) REFERENCES
        erpx_sales_write.quote_workflows (tenant_id, quote_id) ON DELETE RESTRICT,
    CHECK (sales_approver <> finance_approver),
    CHECK (superseded_at IS NULL OR superseded_at >= approved_at)
);

CREATE TABLE erpx_sales_write.quote_approval_rejections (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    quote_id UUID NOT NULL,
    quote_version BIGINT NOT NULL CHECK (quote_version > 0),
    approval_session_id UUID NOT NULL,
    policy_version BIGINT NOT NULL CHECK (policy_version > 0),
    rejected_gate TEXT NOT NULL,
    rejected_by TEXT NOT NULL,
    reason_code TEXT NOT NULL,
    rejected_at TIMESTAMPTZ NOT NULL,
    superseded_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, quote_id, quote_version),
    FOREIGN KEY (tenant_id, quote_id) REFERENCES
        erpx_sales_write.quote_workflows (tenant_id, quote_id) ON DELETE RESTRICT,
    CHECK (superseded_at IS NULL OR superseded_at >= rejected_at)
);

CREATE TABLE erpx_sales_write.quote_accounting_plan_links (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    quote_id UUID NOT NULL,
    quote_version BIGINT NOT NULL CHECK (quote_version > 0),
    accounting_plan_id UUID NOT NULL,
    accounting_plan_version BIGINT NOT NULL CHECK (accounting_plan_version > 0),
    approved_by TEXT NOT NULL,
    approved_at TIMESTAMPTZ NOT NULL,
    superseded_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, quote_id, quote_version),
    FOREIGN KEY (tenant_id, quote_id) REFERENCES
        erpx_sales_write.quote_workflows (tenant_id, quote_id) ON DELETE RESTRICT,
    CHECK (superseded_at IS NULL OR superseded_at >= approved_at)
);

CREATE TABLE erpx_sales_write.quote_pdf_artifacts (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    artifact_id UUID NOT NULL,
    quote_id UUID NOT NULL,
    quote_version BIGINT NOT NULL CHECK (quote_version > 0),
    document_language TEXT NOT NULL CHECK (document_language IN ('thai', 'english')),
    object_key TEXT NOT NULL,
    sha256_digest TEXT NOT NULL CHECK (sha256_digest ~ '^[0-9A-Fa-f]{64}$'),
    byte_length BIGINT NOT NULL CHECK (byte_length > 0),
    generated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, artifact_id),
    UNIQUE (tenant_id, quote_id, quote_version),
    FOREIGN KEY (tenant_id, quote_id) REFERENCES
        erpx_sales_write.quote_workflows (tenant_id, quote_id) ON DELETE RESTRICT,
    CHECK (object_key !~ '(^/|(^|/)\.\.(/|$)|//)')
);

CREATE TABLE erpx_sales_write.quote_external_decisions (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    quote_id UUID NOT NULL,
    quote_version BIGINT NOT NULL CHECK (quote_version > 0),
    artifact_id UUID NOT NULL,
    decision TEXT NOT NULL CHECK (decision IN ('accepted', 'rejected')),
    evidence_reference TEXT NOT NULL,
    attachment_object_key TEXT,
    recorded_by TEXT NOT NULL,
    decided_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, quote_id, quote_version),
    FOREIGN KEY (tenant_id, artifact_id) REFERENCES
        erpx_sales_write.quote_pdf_artifacts (tenant_id, artifact_id) ON DELETE RESTRICT,
    CHECK (attachment_object_key IS NULL OR attachment_object_key !~ '(^/|(^|/)\.\.(/|$)|//)')
);

CREATE OR REPLACE FUNCTION erpx_sales_write.reject_immutable_quote_artifact_change()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'quote PDF and external-decision evidence is immutable';
END
$$;

CREATE TRIGGER quote_pdf_artifacts_immutable
    BEFORE UPDATE OR DELETE ON erpx_sales_write.quote_pdf_artifacts
    FOR EACH ROW EXECUTE FUNCTION erpx_sales_write.reject_immutable_quote_artifact_change();

CREATE TRIGGER quote_external_decisions_immutable
    BEFORE UPDATE OR DELETE ON erpx_sales_write.quote_external_decisions
    FOR EACH ROW EXECUTE FUNCTION erpx_sales_write.reject_immutable_quote_artifact_change();

CREATE TABLE erpx_finance_write.accounting_plans (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    accounting_plan_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    quote_id UUID NOT NULL,
    quote_version BIGINT NOT NULL CHECK (quote_version > 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    quote_net_minor BIGINT NOT NULL CHECK (quote_net_minor > 0),
    quote_tax_minor BIGINT NOT NULL CHECK (quote_tax_minor >= 0),
    dimensions JSONB NOT NULL,
    tax_profile_id UUID NOT NULL,
    tax_profile_version BIGINT NOT NULL CHECK (tax_profile_version > 0),
    tax_classification TEXT NOT NULL CHECK (tax_classification IN ('standard_vat', 'zero_rated', 'exempt')),
    tax_rate_basis_points INTEGER NOT NULL CHECK (tax_rate_basis_points BETWEEN 0 AND 10000),
    tax_authority_ref TEXT NOT NULL,
    accounts_receivable_id UUID NOT NULL,
    output_vat_id UUID NOT NULL,
    contract_liability_id UUID NOT NULL,
    revenue_id UUID NOT NULL,
    preparer TEXT NOT NULL,
    sales_approver TEXT NOT NULL,
    finance_approver TEXT,
    approval_session_id UUID,
    suggestion_id UUID,
    suggestion_payload_digest TEXT CHECK (suggestion_payload_digest ~ '^[0-9A-Fa-f]{64}$'),
    suggestion_output_digest TEXT CHECK (suggestion_output_digest ~ '^[0-9A-Fa-f]{64}$'),
    suggestion_provider_label TEXT,
    plan_status TEXT NOT NULL CHECK (plan_status IN ('draft', 'submitted', 'approved', 'rejected', 'superseded')),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, accounting_plan_id),
    UNIQUE (tenant_id, quote_id, quote_version),
    CHECK (updated_at >= created_at),
    CHECK (preparer <> sales_approver),
    CHECK (finance_approver IS NULL OR (finance_approver <> preparer AND finance_approver <> sales_approver)),
    CHECK (
        (suggestion_id IS NULL AND suggestion_payload_digest IS NULL AND suggestion_output_digest IS NULL AND suggestion_provider_label IS NULL)
        OR (suggestion_id IS NOT NULL AND suggestion_payload_digest IS NOT NULL AND suggestion_output_digest IS NOT NULL AND suggestion_provider_label IS NOT NULL)
    )
);

CREATE TABLE erpx_finance_write.accounting_plan_obligations (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    accounting_plan_id UUID NOT NULL,
    obligation_id UUID NOT NULL,
    allocated_minor BIGINT NOT NULL CHECK (allocated_minor > 0),
    template_code TEXT NOT NULL,
    PRIMARY KEY (tenant_id, accounting_plan_id, obligation_id),
    FOREIGN KEY (tenant_id, accounting_plan_id) REFERENCES
        erpx_finance_write.accounting_plans (tenant_id, accounting_plan_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_finance_write.posting_packets (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    posting_packet_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    accounting_plan_id UUID NOT NULL,
    accounting_plan_version BIGINT NOT NULL CHECK (accounting_plan_version > 0),
    source_event_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    billing_event_id UUID NOT NULL,
    fulfillment_id UUID NOT NULL,
    obligation_id UUID NOT NULL,
    entry_date DATE NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    net_minor BIGINT NOT NULL CHECK (net_minor > 0),
    tax_minor BIGINT NOT NULL CHECK (tax_minor >= 0),
    fulfilled_minor BIGINT NOT NULL CHECK (fulfilled_minor > 0),
    tax_profile_id UUID NOT NULL,
    tax_profile_version BIGINT NOT NULL CHECK (tax_profile_version > 0),
    tax_classification TEXT NOT NULL CHECK (tax_classification IN ('standard_vat', 'zero_rated', 'exempt')),
    tax_rate_basis_points INTEGER NOT NULL CHECK (tax_rate_basis_points BETWEEN 0 AND 10000),
    tax_authority_ref TEXT NOT NULL,
    preparer TEXT NOT NULL,
    sales_approver TEXT NOT NULL,
    final_finance_approver TEXT,
    approval_session_id UUID,
    packet_status TEXT NOT NULL CHECK (packet_status IN (
        'draft', 'submitted', 'approved', 'rejected', 'posted', 'superseded'
    )),
    invoice_id UUID,
    receivable_id UUID,
    billing_journal_id UUID,
    fulfillment_journal_id UUID,
    revenue_schedule_id UUID,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, posting_packet_id),
    UNIQUE (tenant_id, source_event_id),
    FOREIGN KEY (tenant_id, accounting_plan_id) REFERENCES
        erpx_finance_write.accounting_plans (tenant_id, accounting_plan_id) ON DELETE RESTRICT,
    CHECK (fulfilled_minor <= net_minor),
    CHECK (final_finance_approver IS NULL OR (final_finance_approver <> preparer AND final_finance_approver <> sales_approver)),
    CHECK (
        packet_status <> 'posted'
        OR (invoice_id IS NOT NULL AND receivable_id IS NOT NULL AND billing_journal_id IS NOT NULL
            AND fulfillment_journal_id IS NOT NULL AND revenue_schedule_id IS NOT NULL)
    )
);

CREATE TABLE erpx_finance_write.sales_fact_inbox (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    consumer_name TEXT NOT NULL,
    source_event_id UUID NOT NULL,
    source_event_type TEXT NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    global_sequence BIGINT NOT NULL CHECK (global_sequence > 0),
    handoff_id UUID NOT NULL,
    handoff_payload JSONB NOT NULL,
    inbox_status TEXT NOT NULL CHECK (inbox_status IN ('staged', 'applied', 'retryable_failure', 'permanent_failure')),
    failure_code TEXT,
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, consumer_name, source_event_id),
    UNIQUE (tenant_id, consumer_name, global_sequence),
    UNIQUE (tenant_id, handoff_id)
);

CREATE TABLE erpx_finance_write.sales_fact_checkpoints (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    consumer_name TEXT NOT NULL,
    last_global_sequence BIGINT NOT NULL CHECK (last_global_sequence > 0),
    last_source_event_id UUID NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, consumer_name)
);

-- New tables fail closed when no exact tenant has been selected.
DO $$
DECLARE target REGCLASS;
BEGIN
    FOREACH target IN ARRAY ARRAY[
        'erpx_agent_write.suggestion_envelopes'::regclass,
        'erpx_operations_write.approval_sessions'::regclass,
        'erpx_operations_write.approval_session_gates'::regclass,
        'erpx_sales_write.quote_workflows'::regclass,
        'erpx_sales_write.quote_workflow_lines'::regclass,
        'erpx_sales_write.quote_applied_suggestions'::regclass,
        'erpx_sales_write.quote_approval_links'::regclass,
        'erpx_sales_write.quote_approval_rejections'::regclass,
        'erpx_sales_write.quote_accounting_plan_links'::regclass,
        'erpx_sales_write.quote_pdf_artifacts'::regclass,
        'erpx_sales_write.quote_external_decisions'::regclass,
        'erpx_finance_write.accounting_plans'::regclass,
        'erpx_finance_write.accounting_plan_obligations'::regclass,
        'erpx_finance_write.posting_packets'::regclass,
        'erpx_finance_write.sales_fact_inbox'::regclass,
        'erpx_finance_write.sales_fact_checkpoints'::regclass
    ]
    LOOP
        EXECUTE format('ALTER TABLE %s ENABLE ROW LEVEL SECURITY', target);
        EXECUTE format('ALTER TABLE %s FORCE ROW LEVEL SECURITY', target);
        EXECUTE format(
            'CREATE POLICY tenant_isolation ON %s USING (tenant_id = erpx_cqrs_write.current_tenant_id()) WITH CHECK (tenant_id = erpx_cqrs_write.current_tenant_id())',
            target
        );
    END LOOP;
END
$$;
