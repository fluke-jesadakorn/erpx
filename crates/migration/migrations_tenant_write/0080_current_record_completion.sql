-- Generation-v2 completion for current-context first-class records.
-- This migration is additive and never removes an existing database or row.

-- CRM owns the durable, normalized copy of Marketing's published qualified
-- lead evidence. Commands resolve this inbox by tenant and candidate rather
-- than trusting a client-supplied fact or reading Marketing's repository.
ALTER TABLE erpx_crm_write.lead_guards
    ADD COLUMN tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    ADD COLUMN campaign_id UUID NOT NULL,
    ADD COLUMN audience_member_id UUID NOT NULL,
    ADD COLUMN marketing_source_version BIGINT NOT NULL
        CHECK (marketing_source_version > 0);
ALTER TABLE erpx_crm_write.opportunity_guards
    ADD COLUMN tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id();

ALTER TABLE erpx_crm_write.opportunity_guards
    DROP CONSTRAINT opportunity_guards_accepted_lead_id_fkey,
    DROP CONSTRAINT opportunity_guards_pkey;
ALTER TABLE erpx_crm_write.lead_guards
    DROP CONSTRAINT lead_guards_marketing_candidate_id_key,
    DROP CONSTRAINT lead_guards_pkey;
ALTER TABLE erpx_crm_write.lead_guards
    ADD PRIMARY KEY (tenant_id, lead_id),
    ADD UNIQUE (tenant_id, marketing_candidate_id),
    ADD UNIQUE (tenant_id, lead_id, marketing_candidate_id, marketing_source_version);
ALTER TABLE erpx_crm_write.opportunity_guards
    ADD PRIMARY KEY (tenant_id, opportunity_id),
    ADD FOREIGN KEY (tenant_id, accepted_lead_id)
        REFERENCES erpx_crm_write.lead_guards (tenant_id, lead_id);

CREATE TABLE erpx_crm_write.marketing_qualified_lead_evidence (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    marketing_candidate_id UUID NOT NULL,
    campaign_id UUID NOT NULL,
    audience_member_id UUID NOT NULL,
    customer_id UUID,
    source_code TEXT NOT NULL CHECK (octet_length(source_code) BETWEEN 1 AND 255),
    qualification_score_basis_points INTEGER NOT NULL
        CHECK (qualification_score_basis_points BETWEEN 1 AND 10000),
    marketing_source_version BIGINT NOT NULL CHECK (marketing_source_version > 0),
    source_event_id UUID NOT NULL,
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    source_occurred_at TIMESTAMPTZ NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, marketing_candidate_id),
    UNIQUE (tenant_id, source_event_id),
    UNIQUE (tenant_id, marketing_candidate_id, marketing_source_version)
);
CREATE INDEX crm_qualified_lead_evidence_scope
    ON erpx_crm_write.marketing_qualified_lead_evidence
        (tenant_id, scope_key, marketing_source_version, marketing_candidate_id);

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'lead_guards','opportunity_guards','marketing_qualified_lead_evidence'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_crm_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_crm_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY crm_write_tenant_isolation ON erpx_crm_write.%I '
            'USING (tenant_id = erpx_cqrs_write.current_tenant_id()) '
            'WITH CHECK (tenant_id = erpx_cqrs_write.current_tenant_id())',
            table_name
        );
    END LOOP;
END;
$$;

-- Identity governance roots are normalized separately from principal/role
-- snapshots so an access review or emergency session has its own lifecycle,
-- receipt, audit evidence, and first-class record identity.
CREATE TABLE erpx_authority_write.access_reviews (
    tenant_id UUID NOT NULL DEFAULT erpx_authority_write.current_tenant_id(),
    access_review_id UUID NOT NULL,
    review_version BIGINT NOT NULL CHECK (review_version > 0),
    review_scope JSONB NOT NULL,
    reviewer_ref TEXT NOT NULL CHECK (octet_length(reviewer_ref) BETWEEN 1 AND 256),
    due_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    changed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, access_review_id),
    UNIQUE (tenant_id, access_review_id, review_version)
);
CREATE TABLE erpx_authority_write.access_review_items (
    tenant_id UUID NOT NULL DEFAULT erpx_authority_write.current_tenant_id(),
    access_review_id UUID NOT NULL,
    assignment_id UUID NOT NULL,
    principal_id UUID NOT NULL,
    principal_kind TEXT NOT NULL CHECK (
        principal_kind IN ('human','service','agent','human_group','agent_pool')
    ),
    owner_ref TEXT NOT NULL CHECK (octet_length(owner_ref) BETWEEN 1 AND 256),
    grantor_ref TEXT NOT NULL CHECK (octet_length(grantor_ref) BETWEEN 1 AND 256),
    decision TEXT CHECK (decision IN ('retain','revoke')),
    decided_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, access_review_id, assignment_id),
    FOREIGN KEY (tenant_id, access_review_id)
        REFERENCES erpx_authority_write.access_reviews (tenant_id, access_review_id)
        ON DELETE CASCADE,
    CHECK ((decision IS NULL) = (decided_at IS NULL))
);
CREATE TABLE erpx_authority_write.break_glass_sessions (
    tenant_id UUID NOT NULL DEFAULT erpx_authority_write.current_tenant_id(),
    break_glass_session_id UUID NOT NULL,
    session_version BIGINT NOT NULL CHECK (session_version > 0),
    principal_id UUID NOT NULL,
    session_scope JSONB NOT NULL,
    reason TEXT NOT NULL CHECK (octet_length(reason) BETWEEN 1 AND 256),
    requester_ref TEXT NOT NULL CHECK (octet_length(requester_ref) BETWEEN 1 AND 256),
    approver_ref TEXT,
    requested_at TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    activated_at TIMESTAMPTZ,
    closed_at TIMESTAMPTZ,
    after_reviewed_at TIMESTAMPTZ,
    session_status TEXT NOT NULL CHECK (
        session_status IN ('requested','active','closed','expired')
    ),
    changed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, break_glass_session_id),
    UNIQUE (tenant_id, break_glass_session_id, session_version),
    CHECK (valid_until > requested_at)
);
CREATE TABLE erpx_authority_write.identity_governance_command_receipts (
    tenant_id UUID NOT NULL DEFAULT erpx_authority_write.current_tenant_id(),
    idempotency_key TEXT NOT NULL CHECK (octet_length(idempotency_key) BETWEEN 1 AND 256),
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash) = 32),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, idempotency_key),
    UNIQUE (tenant_id, command_id)
);
CREATE TABLE erpx_authority_write.identity_governance_audit_log (
    tenant_id UUID NOT NULL DEFAULT erpx_authority_write.current_tenant_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    actor JSONB NOT NULL,
    authority_scope JSONB NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (
        record_type IN ('identity.access-review','identity.break-glass-session')
    ),
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    action TEXT NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, audit_id),
    UNIQUE (tenant_id, command_id)
);

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'access_reviews','access_review_items','break_glass_sessions',
        'identity_governance_command_receipts','identity_governance_audit_log'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_authority_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_authority_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY identity_governance_tenant_isolation ON erpx_authority_write.%I '
            'USING (tenant_id = erpx_authority_write.current_tenant_id()) '
            'WITH CHECK (tenant_id = erpx_authority_write.current_tenant_id())',
            table_name
        );
    END LOOP;
END;
$$;

-- D13 Batch B/F: People & Payroll owns employment and payslip lifecycle rows.
-- These are normalized child authorities derived inside the same guarded UoW
-- as the employee/payroll aggregate; they are not aggregate JSON aliases.
CREATE TABLE erpx_people_payroll_write.employment_records (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    employment_id UUID NOT NULL,
    employee_id UUID NOT NULL,
    source_employee_version BIGINT NOT NULL CHECK (source_employee_version > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    hired_on DATE NOT NULL,
    terminated_on DATE,
    employment_status TEXT NOT NULL CHECK (employment_status IN (
        'draft','active','on_leave','suspended','terminated'
    )),
    changed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, employment_id),
    UNIQUE (tenant_id, employee_id),
    UNIQUE (tenant_id, employment_id, source_employee_version),
    CHECK (
        (employment_status = 'terminated' AND terminated_on IS NOT NULL
            AND terminated_on >= hired_on)
        OR (employment_status <> 'terminated' AND terminated_on IS NULL)
    )
);
CREATE TABLE erpx_people_payroll_write.payslip_records (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    payslip_id UUID NOT NULL,
    payroll_run_id UUID NOT NULL,
    employee_id UUID NOT NULL,
    source_payroll_version BIGINT NOT NULL CHECK (source_payroll_version > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    period_code TEXT NOT NULL,
    starts_on DATE NOT NULL,
    ends_on DATE NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    gross_minor BIGINT NOT NULL CHECK (gross_minor >= 0),
    deductions_minor BIGINT NOT NULL CHECK (deductions_minor >= 0),
    net_minor BIGINT NOT NULL CHECK (net_minor >= 0),
    payslip_status TEXT NOT NULL CHECK (payslip_status IN (
        'draft','review','approved','posted','cancelled'
    )),
    changed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, payslip_id),
    UNIQUE (tenant_id, payroll_run_id, employee_id),
    UNIQUE (tenant_id, payslip_id, source_payroll_version),
    CHECK (ends_on >= starts_on),
    CHECK (gross_minor - deductions_minor = net_minor)
);
CREATE INDEX employment_records_scope_status
    ON erpx_people_payroll_write.employment_records
        (tenant_id, scope_key, employment_status, employment_id);
CREATE INDEX payslip_records_scope_status
    ON erpx_people_payroll_write.payslip_records
        (tenant_id, scope_key, payslip_status, payroll_run_id, employee_id);

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY['employment_records','payslip_records'] LOOP
        EXECUTE format('ALTER TABLE erpx_people_payroll_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_people_payroll_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY people_completion_tenant_isolation ON erpx_people_payroll_write.%I '
            'USING (tenant_id = erpx_cqrs_write.current_tenant_id()) '
            'WITH CHECK (tenant_id = erpx_cqrs_write.current_tenant_id())',
            table_name
        );
    END LOOP;
END;
$$;

-- D13 Batch B/F: Legal hold is a governed entity inside LegalMatter. The
-- distinct opaque identity and normalized row follow place/release atomically.
CREATE TABLE erpx_legal_write.legal_hold_records (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    legal_hold_id UUID NOT NULL,
    matter_id UUID NOT NULL,
    source_matter_version BIGINT NOT NULL CHECK (source_matter_version > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    hold_active BOOLEAN NOT NULL,
    reason TEXT NOT NULL,
    placed_by TEXT NOT NULL,
    placed_at TIMESTAMPTZ NOT NULL,
    released_by TEXT,
    released_at TIMESTAMPTZ,
    changed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, legal_hold_id),
    UNIQUE (tenant_id, matter_id),
    UNIQUE (tenant_id, legal_hold_id, source_matter_version),
    CHECK (
        (hold_active AND released_by IS NULL AND released_at IS NULL)
        OR (NOT hold_active AND released_by IS NOT NULL
            AND released_at IS NOT NULL AND released_at >= placed_at)
    )
);
CREATE INDEX legal_hold_records_scope_active
    ON erpx_legal_write.legal_hold_records
        (tenant_id, scope_key, hold_active, legal_hold_id);
ALTER TABLE erpx_legal_write.legal_hold_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_legal_write.legal_hold_records FORCE ROW LEVEL SECURITY;
CREATE POLICY legal_hold_record_tenant_isolation
    ON erpx_legal_write.legal_hold_records
    USING (tenant_id = erpx_cqrs_write.current_tenant_id())
    WITH CHECK (tenant_id = erpx_cqrs_write.current_tenant_id());

-- D13 Batch G: Marketing-owned first-class campaign, content, audience
-- activation and consent-purpose lifecycles.
CREATE TABLE erpx_marketing_write.business_records (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    record_kind TEXT NOT NULL CHECK (record_kind IN (
        'campaign','content','audience_activation','consent_purpose'
    )),
    record_scope JSONB NOT NULL,
    record_title TEXT NOT NULL CHECK (octet_length(record_title) BETWEEN 1 AND 255),
    record_status TEXT NOT NULL CHECK (record_status IN (
        'draft','proposed','approved','active','paused','running','published',
        'completed','cancelled','retired'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,record_id),
    UNIQUE (tenant_id,record_id,record_version),
    CHECK (updated_at >= created_at)
);
CREATE TABLE erpx_marketing_write.business_record_receipts (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash)=32),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,idempotency_key),
    UNIQUE (tenant_id,command_id)
);
CREATE TABLE erpx_marketing_write.business_record_audit (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    actor JSONB NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('create','transition')),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,audit_id),
    UNIQUE (tenant_id,command_id)
);
DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'business_records','business_record_receipts','business_record_audit'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_marketing_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_marketing_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY marketing_record_tenant_isolation ON erpx_marketing_write.%I '
            'USING (tenant_id=erpx_cqrs_write.current_tenant_id()) '
            'WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id())', table_name
        );
    END LOOP;
END;
$$;

-- D13 Batch G: CRM-owned first-class activity and forecast lifecycles.
CREATE TABLE erpx_crm_write.business_records (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    record_kind TEXT NOT NULL CHECK (record_kind IN (
        'activity','forecast'
    )),
    record_scope JSONB NOT NULL,
    record_title TEXT NOT NULL CHECK (octet_length(record_title) BETWEEN 1 AND 255),
    record_status TEXT NOT NULL CHECK (record_status IN (
        'draft','planned','in_progress','submitted','approved','completed',
        'cancelled','superseded'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,record_id),
    UNIQUE (tenant_id,record_id,record_version),
    CHECK (updated_at >= created_at)
);
CREATE TABLE erpx_crm_write.business_record_receipts (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash)=32),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,idempotency_key),
    UNIQUE (tenant_id,command_id)
);
CREATE TABLE erpx_crm_write.business_record_audit (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    actor JSONB NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('create','transition')),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,audit_id),
    UNIQUE (tenant_id,command_id)
);
DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'business_records','business_record_receipts','business_record_audit'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_crm_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_crm_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY crm_record_tenant_isolation ON erpx_crm_write.%I '
            'USING (tenant_id=erpx_cqrs_write.current_tenant_id()) '
            'WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id())', table_name
        );
    END LOOP;
END;
$$;



-- D13 Batch G: Finance-owned first-class activity and forecast lifecycles.
CREATE TABLE erpx_finance_write.business_records (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    record_kind TEXT NOT NULL CHECK (record_kind IN (
        'credit_note','debit_note','payment','reconciliation'
    )),
    record_scope JSONB NOT NULL,
    record_title TEXT NOT NULL CHECK (octet_length(record_title) BETWEEN 1 AND 255),
    record_status TEXT NOT NULL CHECK (record_status IN (
        'draft','pending','open','posted','authorized','matched','applied',
        'settled','closed','failed','cancelled','void'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,record_id),
    UNIQUE (tenant_id,record_id,record_version),
    CHECK (updated_at >= created_at)
);
CREATE TABLE erpx_finance_write.business_record_receipts (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash)=32),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,idempotency_key),
    UNIQUE (tenant_id,command_id)
);
CREATE TABLE erpx_finance_write.business_record_audit (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    actor JSONB NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('create','transition')),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,audit_id),
    UNIQUE (tenant_id,command_id)
);
DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'business_records','business_record_receipts','business_record_audit'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_finance_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_finance_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY finance_record_tenant_isolation ON erpx_finance_write.%I '
            'USING (tenant_id=erpx_cqrs_write.current_tenant_id()) '
            'WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id())', table_name
        );
    END LOOP;
END;
$$;





-- D13 Batch G: Procurement-owned first-class activity and forecast lifecycles.
CREATE TABLE erpx_procurement_write.business_records (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    record_kind TEXT NOT NULL CHECK (record_kind IN (
        'requisition','rfq','receipt','supplier_return'
    )),
    record_scope JSONB NOT NULL,
    record_title TEXT NOT NULL CHECK (octet_length(record_title) BETWEEN 1 AND 255),
    record_status TEXT NOT NULL CHECK (record_status IN (
        'draft','expected','proposed','submitted','approved','rejected','issued',
        'received','accepted','authorized','shipped','closed','cancelled'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,record_id),
    UNIQUE (tenant_id,record_id,record_version),
    CHECK (updated_at >= created_at)
);
CREATE TABLE erpx_procurement_write.business_record_receipts (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash)=32),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,idempotency_key),
    UNIQUE (tenant_id,command_id)
);
CREATE TABLE erpx_procurement_write.business_record_audit (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    actor JSONB NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('create','transition')),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,audit_id),
    UNIQUE (tenant_id,command_id)
);
DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'business_records','business_record_receipts','business_record_audit'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_procurement_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_procurement_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY procurement_record_tenant_isolation ON erpx_procurement_write.%I '
            'USING (tenant_id=erpx_cqrs_write.current_tenant_id()) '
            'WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id())', table_name
        );
    END LOOP;
END;
$$;







-- D13 Batch B/F2: genuine owner lifecycles for the remaining current families.
CREATE SCHEMA IF NOT EXISTS erpx_customer_authority_write;
CREATE SCHEMA IF NOT EXISTS erpx_customer_360_write;

CREATE TABLE erpx_customer_authority_write.business_authority_records (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    record_id UUID NOT NULL, record_kind TEXT NOT NULL CHECK (record_kind IN ('authority_grant','support_access')),
    record_version BIGINT NOT NULL CHECK (record_version>0), customer_id UUID NOT NULL,
    customer_account_id UUID NOT NULL, support_case_id UUID, tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL, evidence_sha256 TEXT NOT NULL CHECK (evidence_sha256~'^sha256:[0-9A-Fa-f]{64}$'),
    record_status TEXT NOT NULL CHECK (record_status IN ('proposed','active','revoked','expired')),
    valid_until TIMESTAMPTZ NOT NULL, created_by TEXT NOT NULL, revoked_by TEXT,
    created_at TIMESTAMPTZ NOT NULL, updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY(tenant_id,record_id), UNIQUE(tenant_id,record_id,record_version),
    CHECK ((record_kind='authority_grant' AND support_case_id IS NULL) OR (record_kind='support_access' AND support_case_id IS NOT NULL)),
    CHECK (updated_at>=created_at AND valid_until>created_at)
);
CREATE INDEX business_authority_record_scope ON erpx_customer_authority_write.business_authority_records(tenant_id,scope_key,record_kind,record_status,record_id);

CREATE TABLE erpx_customer_360_write.owned_records (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(), record_id UUID NOT NULL,
    record_kind TEXT NOT NULL CHECK(record_kind IN ('profile','note','merge')),
    record_version BIGINT NOT NULL CHECK(record_version>0), tenant_scope JSONB NOT NULL, scope_key TEXT NOT NULL,
    customer_account_id UUID NOT NULL, related_customer_account_id UUID, source_version BIGINT NOT NULL CHECK(source_version>0),
    title TEXT NOT NULL CHECK(octet_length(title) BETWEEN 1 AND 255), content_object_ref TEXT,
    content_sha256 TEXT NOT NULL CHECK(content_sha256~'^sha256:[0-9A-Fa-f]{64}$'),
    record_status TEXT NOT NULL CHECK(record_status IN ('draft','active','closed','cancelled')),
    created_by TEXT NOT NULL, closed_by TEXT, created_at TIMESTAMPTZ NOT NULL, updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY(tenant_id,record_id), UNIQUE(tenant_id,record_id,record_version),
    CHECK ((record_kind='merge' AND related_customer_account_id IS NOT NULL AND related_customer_account_id<>customer_account_id)
        OR (record_kind<>'merge' AND related_customer_account_id IS NULL)), CHECK(updated_at>=created_at)
);
CREATE INDEX customer_360_owned_record_scope ON erpx_customer_360_write.owned_records(tenant_id,scope_key,record_kind,record_status,record_id);

CREATE TABLE erpx_support_write.platform_cases (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(), case_id UUID NOT NULL,
    case_version BIGINT NOT NULL CHECK(case_version>0), tenant_scope JSONB NOT NULL, scope_key TEXT NOT NULL,
    case_code TEXT NOT NULL, title TEXT NOT NULL, source_record_id UUID, source_record_type TEXT,
    source_version BIGINT CHECK(source_version>0), evidence_sha256 TEXT NOT NULL CHECK(evidence_sha256~'^sha256:[0-9A-Fa-f]{64}$'),
    case_status TEXT NOT NULL CHECK(case_status IN ('open','investigating','waiting','resolved','closed','cancelled')),
    opened_by TEXT NOT NULL, resolved_by TEXT, created_at TIMESTAMPTZ NOT NULL, updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY(tenant_id,case_id), UNIQUE(tenant_id,case_id,case_version), UNIQUE(tenant_id,case_code),
    CHECK((source_record_id IS NULL)=(source_version IS NULL)), CHECK((source_record_id IS NULL)=(source_record_type IS NULL)), CHECK(updated_at>=created_at)
);
CREATE INDEX platform_case_scope_status ON erpx_support_write.platform_cases(tenant_id,scope_key,case_status,case_id);

CREATE TABLE erpx_people_payroll_write.time_records (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(), time_record_id UUID NOT NULL,
    time_record_version BIGINT NOT NULL CHECK(time_record_version>0), tenant_scope JSONB NOT NULL, scope_key TEXT NOT NULL,
    employee_id UUID NOT NULL, employment_record_id UUID NOT NULL, work_date DATE NOT NULL,
    minutes INTEGER NOT NULL CHECK(minutes BETWEEN 1 AND 1440), cost_center TEXT NOT NULL,
    evidence_sha256 TEXT NOT NULL CHECK(evidence_sha256~'^sha256:[0-9A-Fa-f]{64}$'),
    time_status TEXT NOT NULL CHECK(time_status IN ('draft','submitted','approved','posted','voided')),
    submitted_by TEXT NOT NULL, approved_by TEXT, posted_by TEXT, created_at TIMESTAMPTZ NOT NULL, updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY(tenant_id,time_record_id), UNIQUE(tenant_id,time_record_id,time_record_version), CHECK(updated_at>=created_at)
);
CREATE INDEX time_record_employee_date ON erpx_people_payroll_write.time_records(tenant_id,scope_key,employee_id,work_date,time_record_id);

CREATE TABLE erpx_legal_write.legal_cases (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(), case_id UUID NOT NULL,
    case_version BIGINT NOT NULL CHECK(case_version>0), tenant_scope JSONB NOT NULL, scope_key TEXT NOT NULL,
    case_reference TEXT NOT NULL, title TEXT NOT NULL, related_matter_id UUID, source_version BIGINT CHECK(source_version>0),
    evidence_sha256 TEXT NOT NULL CHECK(evidence_sha256~'^sha256:[0-9A-Fa-f]{64}$'),
    case_status TEXT NOT NULL CHECK(case_status IN ('proposed','open','on-hold','resolved','closed','cancelled')),
    owner_ref TEXT NOT NULL, closed_by TEXT, retention_until TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL, updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY(tenant_id,case_id), UNIQUE(tenant_id,case_id,case_version), UNIQUE(tenant_id,case_reference),
    CHECK((related_matter_id IS NULL)=(source_version IS NULL)), CHECK(updated_at>=created_at AND retention_until>created_at)
);
CREATE INDEX legal_case_scope_status ON erpx_legal_write.legal_cases(tenant_id,scope_key,case_status,case_id);

DO $$ DECLARE item RECORD; BEGIN
    FOR item IN SELECT * FROM (VALUES
        ('erpx_customer_authority_write','business_authority_records','customer_authority_completion_tenant_isolation'),
        ('erpx_customer_360_write','owned_records','customer_360_owned_tenant_isolation'),
        ('erpx_support_write','platform_cases','support_platform_case_tenant_isolation'),
        ('erpx_people_payroll_write','time_records','people_time_record_tenant_isolation'),
        ('erpx_legal_write','legal_cases','legal_case_tenant_isolation')) AS v(schema_name,table_name,policy_name)
    LOOP
        EXECUTE format('ALTER TABLE %I.%I ENABLE ROW LEVEL SECURITY',item.schema_name,item.table_name);
        EXECUTE format('ALTER TABLE %I.%I FORCE ROW LEVEL SECURITY',item.schema_name,item.table_name);
        EXECUTE format('CREATE POLICY %I ON %I.%I USING (tenant_id=erpx_cqrs_write.current_tenant_id()) WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id())',item.policy_name,item.schema_name,item.table_name);
    END LOOP;
END $$;

-- D13 Batch H: Assets-owned transfer, depreciation, and disposal lifecycles.
CREATE TABLE erpx_assets_write.business_records (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    record_kind TEXT NOT NULL CHECK (record_kind IN (
        'transfer','depreciation_run','disposal'
    )),
    record_scope JSONB NOT NULL,
    record_title TEXT NOT NULL CHECK (octet_length(record_title) BETWEEN 1 AND 255),
    record_status TEXT NOT NULL CHECK (record_status IN (
        'draft','proposed','calculated','approved','rejected','in_transit',
        'received','posted','disposed','closed','cancelled'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,record_id),
    UNIQUE (tenant_id,record_id,record_version),
    CHECK (updated_at >= created_at)
);
CREATE TABLE erpx_assets_write.business_record_receipts (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash)=32),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,idempotency_key),
    UNIQUE (tenant_id,command_id)
);
CREATE TABLE erpx_assets_write.business_record_audit (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    actor JSONB NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('create','transition')),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,audit_id),
    UNIQUE (tenant_id,command_id)
);
DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'business_records','business_record_receipts','business_record_audit'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_assets_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_assets_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY assets_record_tenant_isolation ON erpx_assets_write.%I '
            'USING (tenant_id=erpx_cqrs_write.current_tenant_id()) '
            'WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id())', table_name
        );
    END LOOP;
END;
$$;

-- D13 Batch I: Maintenance-owned plan, inspection, and request lifecycles.
CREATE TABLE erpx_maintenance_write.business_records (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    record_kind TEXT NOT NULL CHECK (record_kind IN (
        'plan','inspection','request'
    )),
    record_scope JSONB NOT NULL,
    record_title TEXT NOT NULL CHECK (octet_length(record_title) BETWEEN 1 AND 255),
    record_status TEXT NOT NULL CHECK (record_status IN (
        'draft','scheduled','open','triaged','approved','rejected','active',
        'suspended','retired','in_progress','passed','failed','converted','closed','cancelled'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,record_id),
    UNIQUE (tenant_id,record_id,record_version),
    CHECK (updated_at >= created_at)
);
CREATE TABLE erpx_maintenance_write.business_record_receipts (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash)=32),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,idempotency_key),
    UNIQUE (tenant_id,command_id)
);
CREATE TABLE erpx_maintenance_write.business_record_audit (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    actor JSONB NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('create','transition')),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,audit_id),
    UNIQUE (tenant_id,command_id)
);
DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'business_records','business_record_receipts','business_record_audit'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_maintenance_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_maintenance_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY maintenance_record_tenant_isolation ON erpx_maintenance_write.%I '
            'USING (tenant_id=erpx_cqrs_write.current_tenant_id()) '
            'WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id())', table_name
        );
    END LOOP;
END;
$$;

-- D13 Batch J: Quality-owned plan, nonconformance, and corrective-action lifecycles.
-- Quality's earlier tenant-write guard migration predates the context-owned
-- schema name used by the normalized authority. Fresh generation-two
-- databases therefore create the schema here before its first relation.
CREATE SCHEMA IF NOT EXISTS erpx_quality_write;

CREATE TABLE erpx_quality_write.business_records (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    record_kind TEXT NOT NULL CHECK (record_kind IN (
        'plan','nonconformance','corrective_action'
    )),
    record_scope JSONB NOT NULL,
    record_title TEXT NOT NULL CHECK (octet_length(record_title) BETWEEN 1 AND 255),
    record_status TEXT NOT NULL CHECK (record_status IN (
        'draft','proposed','open','approved','rejected','active','suspended',
        'retired','investigating','dispositioned','in_progress','verified',
        'verification_failed','closed','cancelled'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,record_id),
    UNIQUE (tenant_id,record_id,record_version),
    CHECK (updated_at >= created_at)
);
CREATE TABLE erpx_quality_write.business_record_receipts (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash)=32),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,idempotency_key),
    UNIQUE (tenant_id,command_id)
);
CREATE TABLE erpx_quality_write.business_record_audit (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    actor JSONB NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('create','transition')),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,audit_id),
    UNIQUE (tenant_id,command_id)
);
DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'business_records','business_record_receipts','business_record_audit'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_quality_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_quality_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY quality_record_tenant_isolation ON erpx_quality_write.%I '
            'USING (tenant_id=erpx_cqrs_write.current_tenant_id()) '
            'WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id())', table_name
        );
    END LOOP;
END;
$$;

-- D13 Batch K: Operations-owned operation, case, and approval-work lifecycles.
CREATE TABLE erpx_operations_write.business_records (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    record_kind TEXT NOT NULL CHECK (record_kind IN (
        'operation','case','approval_work'
    )),
    record_scope JSONB NOT NULL,
    record_title TEXT NOT NULL CHECK (octet_length(record_title) BETWEEN 1 AND 255),
    record_status TEXT NOT NULL CHECK (record_status IN (
        'planned','proposed','open','scheduled','pending','approved','rejected',
        'investigating','in_progress','waiting','resolved','completed','expired',
        'closed','cancelled'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,record_id),
    UNIQUE (tenant_id,record_id,record_version),
    CHECK (updated_at >= created_at)
);
CREATE TABLE erpx_operations_write.business_record_receipts (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash)=32),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,idempotency_key),
    UNIQUE (tenant_id,command_id)
);
CREATE TABLE erpx_operations_write.business_record_audit (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    actor JSONB NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('create','transition')),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,audit_id),
    UNIQUE (tenant_id,command_id)
);
DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'business_records','business_record_receipts','business_record_audit'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_operations_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_operations_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY operations_record_tenant_isolation ON erpx_operations_write.%I '
            'USING (tenant_id=erpx_cqrs_write.current_tenant_id()) '
            'WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id())', table_name
        );
    END LOOP;
END;
$$;

-- D13 Batch L: Commercial-owned account and adjustment lifecycles.
CREATE TABLE erpx_commercial_write.business_records (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    record_kind TEXT NOT NULL CHECK (record_kind IN (
        'account','adjustment'
    )),
    record_scope JSONB NOT NULL,
    record_title TEXT NOT NULL CHECK (octet_length(record_title) BETWEEN 1 AND 255),
    record_status TEXT NOT NULL CHECK (record_status IN (
        'proposed','pending','active','suspended','approved','rejected','applied',
        'closed','cancelled'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,record_id),
    UNIQUE (tenant_id,record_id,record_version),
    CHECK (updated_at >= created_at)
);
CREATE TABLE erpx_commercial_write.business_record_receipts (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash)=32),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,idempotency_key),
    UNIQUE (tenant_id,command_id)
);
CREATE TABLE erpx_commercial_write.business_record_audit (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    actor JSONB NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('create','transition')),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,audit_id),
    UNIQUE (tenant_id,command_id)
);
DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'business_records','business_record_receipts','business_record_audit'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_commercial_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_commercial_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY commercial_record_tenant_isolation ON erpx_commercial_write.%I '
            'USING (tenant_id=erpx_cqrs_write.current_tenant_id()) '
            'WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id())', table_name
        );
    END LOOP;
END;
$$;

-- D13 Batch M: POS-owned register-session and cash-movement lifecycles.
CREATE TABLE erpx_pos_write.business_records (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    record_kind TEXT NOT NULL CHECK (record_kind IN (
        'register_session','cash_movement'
    )),
    record_scope JSONB NOT NULL,
    record_title TEXT NOT NULL CHECK (octet_length(record_title) BETWEEN 1 AND 255),
    record_status TEXT NOT NULL CHECK (record_status IN (
        'opening','open','suspended','reconciling','draft','approved','rejected',
        'posted','reversed','closed','cancelled'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,record_id),
    UNIQUE (tenant_id,record_id,record_version),
    CHECK (updated_at >= created_at)
);
CREATE TABLE erpx_pos_write.business_record_receipts (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash)=32),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,idempotency_key),
    UNIQUE (tenant_id,command_id)
);
CREATE TABLE erpx_pos_write.business_record_audit (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    actor JSONB NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('create','transition')),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,audit_id),
    UNIQUE (tenant_id,command_id)
);
DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'business_records','business_record_receipts','business_record_audit'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_pos_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_pos_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY pos_record_tenant_isolation ON erpx_pos_write.%I '
            'USING (tenant_id=erpx_cqrs_write.current_tenant_id()) '
            'WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id())', table_name
        );
    END LOOP;
END;
$$;

-- D13 Batch N: Sales-owned return and fulfillment-handoff lifecycles.
CREATE TABLE erpx_sales_write.business_records (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    record_kind TEXT NOT NULL CHECK (record_kind IN (
        'return','fulfillment_handoff'
    )),
    record_scope JSONB NOT NULL,
    record_title TEXT NOT NULL CHECK (octet_length(record_title) BETWEEN 1 AND 255),
    record_status TEXT NOT NULL CHECK (record_status IN (
        'requested','proposed','accepted','approved','authorized','rejected',
        'received','inspected','in_progress','completed','failed','closed','cancelled'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,record_id),
    UNIQUE (tenant_id,record_id,record_version),
    CHECK (updated_at >= created_at)
);
CREATE TABLE erpx_sales_write.business_record_receipts (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash)=32),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,idempotency_key),
    UNIQUE (tenant_id,command_id)
);
CREATE TABLE erpx_sales_write.business_record_audit (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    actor JSONB NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('create','transition')),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,audit_id),
    UNIQUE (tenant_id,command_id)
);
DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'business_records','business_record_receipts','business_record_audit'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_sales_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_sales_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY sales_record_tenant_isolation ON erpx_sales_write.%I '
            'USING (tenant_id=erpx_cqrs_write.current_tenant_id()) '
            'WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id())', table_name
        );
    END LOOP;
END;
$$;

-- D13 Batch O: Inventory-owned document, transfer, count, lot, and serial lifecycles.
CREATE TABLE erpx_inventory_write.business_records (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    record_kind TEXT NOT NULL CHECK (record_kind IN (
        'document','transfer','count','lot','serial'
    )),
    record_scope JSONB NOT NULL,
    record_title TEXT NOT NULL CHECK (octet_length(record_title) BETWEEN 1 AND 255),
    record_status TEXT NOT NULL CHECK (record_status IN (
        'draft','proposed','planned','active','registered','approved','rejected',
        'posted','reversed','in_transit','received','in_progress','reconciled',
        'quarantined','expired','in_stock','allocated','shipped','returned',
        'retired','closed','cancelled'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,record_id),
    UNIQUE (tenant_id,record_id,record_version),
    CHECK (updated_at >= created_at)
);
CREATE TABLE erpx_inventory_write.business_record_receipts (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash)=32),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,idempotency_key),
    UNIQUE (tenant_id,command_id)
);
CREATE TABLE erpx_inventory_write.business_record_audit (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    actor JSONB NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('create','transition')),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,audit_id),
    UNIQUE (tenant_id,command_id)
);
DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'business_records','business_record_receipts','business_record_audit'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_inventory_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_inventory_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY inventory_record_tenant_isolation ON erpx_inventory_write.%I '
            'USING (tenant_id=erpx_cqrs_write.current_tenant_id()) '
            'WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id())', table_name
        );
    END LOOP;
END;
$$;

-- Final generation-two role marker: erpx_meta.schema_manifest,
-- D13 Batch O2: tenant-bound derived authority for StockItem-owned reservations.
CREATE TABLE erpx_inventory_write.reservation_records (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    reservation_id UUID NOT NULL,
    stock_item_id UUID NOT NULL,
    source_item_version BIGINT NOT NULL CHECK (source_item_version > 0),
    tenant_scope JSONB NOT NULL,
    warehouse_id UUID NOT NULL,
    item_code TEXT NOT NULL,
    quantity BIGINT NOT NULL CHECK (quantity > 0),
    demand_reference TEXT NOT NULL,
    reservation_status TEXT NOT NULL CHECK (reservation_status IN ('active','released')),
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,reservation_id),
    UNIQUE (tenant_id,reservation_id,source_item_version)
);
CREATE INDEX inventory_reservation_record_scope
    ON erpx_inventory_write.reservation_records
        (tenant_id,stock_item_id,reservation_status,reservation_id);
ALTER TABLE erpx_inventory_write.reservation_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_inventory_write.reservation_records FORCE ROW LEVEL SECURITY;
CREATE POLICY inventory_reservation_record_tenant_isolation
    ON erpx_inventory_write.reservation_records
    USING (tenant_id=erpx_cqrs_write.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id());

-- Final generation-two role marker: erpx_meta.schema_manifest,
-- schema_generation=2, database_role=tenant_write, migration_set=tenant-write-v2.
UPDATE erpx_meta.schema_manifest
SET installed_at = now()
WHERE singleton AND schema_generation = 2
  AND database_role = 'tenant_write' AND migration_set = 'tenant-write-v2';
