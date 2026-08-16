-- Generation-v2 current-context projection completion.

ALTER TABLE erpx_crm_read.lead_projection
    ADD COLUMN campaign_id UUID NOT NULL,
    ADD COLUMN audience_member_id UUID NOT NULL,
    ADD COLUMN marketing_source_version BIGINT NOT NULL
        CHECK (marketing_source_version > 0);

CREATE INDEX crm_lead_marketing_provenance
    ON erpx_crm_read.lead_projection
        (tenant_id, generation, campaign_id, audience_member_id,
         marketing_source_version, lead_id);

-- Identity governance records are disposable projections sourced only from
-- committed governance snapshot facts. They never fall back to the write DB.
CREATE TABLE erpx_identity_access_read.governance_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'identity_access.governance'
        CHECK (projection_name = 'identity_access.governance'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (
        record_type IN ('identity.access-review','identity.break-glass-session')
    ),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    lifecycle_state TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY (tenant_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (tenant_id,projection_name,generation)
        ON DELETE CASCADE
);
CREATE INDEX identity_governance_projection_scope
    ON erpx_identity_access_read.governance_projection
        (tenant_id,generation,record_type,lifecycle_state,record_id);
ALTER TABLE erpx_identity_access_read.governance_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_identity_access_read.governance_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY identity_governance_read_tenant_isolation
    ON erpx_identity_access_read.governance_projection
    USING (tenant_id=erpx_cqrs_read.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_read.current_tenant_id());

-- D13 Batch B/F: active-generation employment and payslip projections.
CREATE TABLE erpx_people_payroll_read.employment_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'people_payroll.employees'
        CHECK (projection_name = 'people_payroll.employees'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    employment_id UUID NOT NULL,
    employee_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    employee_code TEXT NOT NULL,
    display_name TEXT NOT NULL,
    hired_on DATE NOT NULL,
    terminated_on DATE,
    employment_status TEXT NOT NULL CHECK (employment_status IN (
        'draft','active','on_leave','suspended','terminated'
    )),
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,projection_name,generation,partition_key,employment_id),
    UNIQUE (tenant_id,projection_name,generation,employee_id),
    FOREIGN KEY (tenant_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (tenant_id,projection_name,generation) ON DELETE CASCADE
);
CREATE TABLE erpx_people_payroll_read.payslip_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'people_payroll.payroll_runs'
        CHECK (projection_name = 'people_payroll.payroll_runs'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    payslip_id UUID NOT NULL,
    payroll_run_id UUID NOT NULL,
    employee_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
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
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,projection_name,generation,partition_key,payslip_id),
    UNIQUE (tenant_id,projection_name,generation,payroll_run_id,employee_id),
    FOREIGN KEY (tenant_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (tenant_id,projection_name,generation) ON DELETE CASCADE,
    CHECK (gross_minor - deductions_minor = net_minor)
);
CREATE INDEX employment_projection_scope_status
    ON erpx_people_payroll_read.employment_projection
        (tenant_id,generation,scope_key,employment_status,employment_id);
CREATE INDEX payslip_projection_scope_status
    ON erpx_people_payroll_read.payslip_projection
        (tenant_id,generation,scope_key,payslip_status,payroll_run_id,employee_id);

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY['employment_projection','payslip_projection'] LOOP
        EXECUTE format('ALTER TABLE erpx_people_payroll_read.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_people_payroll_read.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY people_completion_read_tenant_isolation ON erpx_people_payroll_read.%I '
            'USING (tenant_id = erpx_cqrs_read.current_tenant_id()) '
            'WITH CHECK (tenant_id = erpx_cqrs_read.current_tenant_id())',
            table_name
        );
    END LOOP;
END;
$$;

-- D13 Batch B/F: active-generation Legal hold projection.
CREATE TABLE erpx_legal_read.legal_hold_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'legal.matters'
        CHECK (projection_name = 'legal.matters'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    legal_hold_id UUID NOT NULL,
    matter_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    matter_reference TEXT NOT NULL,
    hold_active BOOLEAN NOT NULL,
    reason TEXT NOT NULL,
    placed_by TEXT NOT NULL,
    placed_at TIMESTAMPTZ NOT NULL,
    released_by TEXT,
    released_at TIMESTAMPTZ,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,projection_name,generation,partition_key,legal_hold_id),
    UNIQUE (tenant_id,projection_name,generation,matter_id),
    FOREIGN KEY (tenant_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (tenant_id,projection_name,generation) ON DELETE CASCADE,
    CHECK (
        (hold_active AND released_by IS NULL AND released_at IS NULL)
        OR (NOT hold_active AND released_by IS NOT NULL
            AND released_at IS NOT NULL AND released_at >= placed_at)
    )
);
CREATE INDEX legal_hold_projection_scope_active
    ON erpx_legal_read.legal_hold_projection
        (tenant_id,generation,scope_key,hold_active,legal_hold_id);
ALTER TABLE erpx_legal_read.legal_hold_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_legal_read.legal_hold_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY legal_hold_projection_tenant_isolation
    ON erpx_legal_read.legal_hold_projection
    USING (tenant_id = erpx_cqrs_read.current_tenant_id())
    WITH CHECK (tenant_id = erpx_cqrs_read.current_tenant_id());

-- D13 Batch G: rebuildable Marketing business-record projection.
CREATE TABLE erpx_marketing_read.business_record_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'marketing.business-records'
        CHECK (projection_name='marketing.business-records'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (record_type IN (
        'marketing.campaign','marketing.content','marketing.audience-activation',
        'marketing.consent-purpose'
    )),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence>0),
    tenant_scope JSONB NOT NULL,
    record_title TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY (tenant_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (tenant_id,projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX marketing_business_record_projection_scope
    ON erpx_marketing_read.business_record_projection
        (tenant_id,generation,record_type,lifecycle_state,record_id);
ALTER TABLE erpx_marketing_read.business_record_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_marketing_read.business_record_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY marketing_business_record_read_tenant_isolation
    ON erpx_marketing_read.business_record_projection
    USING (tenant_id=erpx_cqrs_read.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_read.current_tenant_id());

-- D13 Batch A: disposable active-generation Organization sharing projection.
CREATE TABLE erpx_organization_read.sharing_agreement_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'organization.sharing-agreements'
        CHECK (projection_name='organization.sharing-agreements'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    sharing_agreement_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence>0),
    source_scope JSONB NOT NULL,
    target_scope JSONB NOT NULL,
    sharing_status TEXT NOT NULL CHECK (sharing_status IN (
        'proposed','active','rejected','revoked','expired'
    )),
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (
        tenant_id,projection_name,generation,partition_key,sharing_agreement_id
    ),
    FOREIGN KEY (tenant_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (tenant_id,projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX organization_sharing_projection_status
    ON erpx_organization_read.sharing_agreement_projection
        (tenant_id,generation,sharing_status,sharing_agreement_id);
ALTER TABLE erpx_organization_read.sharing_agreement_projection
    ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_organization_read.sharing_agreement_projection
    FORCE ROW LEVEL SECURITY;
CREATE POLICY organization_sharing_projection_tenant_isolation
    ON erpx_organization_read.sharing_agreement_projection
    USING (tenant_id=erpx_cqrs_read.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_read.current_tenant_id());

-- D13 Batch G: rebuildable CRM business-record projection.
CREATE TABLE erpx_crm_read.business_record_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'crm.business-records'
        CHECK (projection_name='crm.business-records'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (record_type IN (
        'crm.activity','crm.forecast'
    )),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence>0),
    tenant_scope JSONB NOT NULL,
    record_title TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY (tenant_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (tenant_id,projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX crm_business_record_projection_scope
    ON erpx_crm_read.business_record_projection
        (tenant_id,generation,record_type,lifecycle_state,record_id);
ALTER TABLE erpx_crm_read.business_record_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_crm_read.business_record_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY crm_business_record_read_tenant_isolation
    ON erpx_crm_read.business_record_projection
    USING (tenant_id=erpx_cqrs_read.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_read.current_tenant_id());

-- D13 Batch G: rebuildable Finance business-record projection.
CREATE TABLE erpx_finance_read.business_record_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'finance.business-records'
        CHECK (projection_name='finance.business-records'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (record_type IN (
        'finance.credit-note','finance.debit-note','finance.payment','finance.reconciliation'
    )),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence>0),
    tenant_scope JSONB NOT NULL,
    record_title TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY (tenant_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (tenant_id,projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX finance_business_record_projection_scope
    ON erpx_finance_read.business_record_projection
        (tenant_id,generation,record_type,lifecycle_state,record_id);
ALTER TABLE erpx_finance_read.business_record_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_finance_read.business_record_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY finance_business_record_read_tenant_isolation
    ON erpx_finance_read.business_record_projection
    USING (tenant_id=erpx_cqrs_read.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_read.current_tenant_id());



-- D13 Batch G: rebuildable Procurement business-record projection.
CREATE TABLE erpx_procurement_read.business_record_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'procurement.business-records'
        CHECK (projection_name='procurement.business-records'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (record_type IN (
        'procurement.requisition','procurement.rfq','procurement.receipt','procurement.supplier-return'
    )),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence>0),
    tenant_scope JSONB NOT NULL,
    record_title TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY (tenant_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (tenant_id,projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX procurement_business_record_projection_scope
    ON erpx_procurement_read.business_record_projection
        (tenant_id,generation,record_type,lifecycle_state,record_id);
ALTER TABLE erpx_procurement_read.business_record_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_procurement_read.business_record_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY procurement_business_record_read_tenant_isolation
    ON erpx_procurement_read.business_record_projection
    USING (tenant_id=erpx_cqrs_read.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_read.current_tenant_id());





-- D13 Batch B/F2: active-generation projections for the remaining current families.
CREATE SCHEMA IF NOT EXISTS erpx_customer_authority_read;
CREATE SCHEMA IF NOT EXISTS erpx_customer_360_read;

CREATE TABLE erpx_customer_authority_read.business_authority_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(), projection_name TEXT NOT NULL DEFAULT 'customer_authority.business-records',
    generation UUID NOT NULL, partition_key TEXT NOT NULL, record_id UUID NOT NULL, record_kind TEXT NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK(source_aggregate_version>0), source_global_sequence BIGINT NOT NULL CHECK(source_global_sequence>0),
    tenant_scope JSONB NOT NULL, scope_key TEXT NOT NULL, customer_id UUID NOT NULL, record_status TEXT NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL, projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY(tenant_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY(tenant_id,projection_name,generation) REFERENCES erpx_cqrs_read.projection_generations(tenant_id,projection_name,generation) ON DELETE CASCADE
);
CREATE TABLE erpx_customer_360_read.owned_record_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(), projection_name TEXT NOT NULL DEFAULT 'customer360.owned-records',
    generation UUID NOT NULL, partition_key TEXT NOT NULL, record_id UUID NOT NULL, record_kind TEXT NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK(source_aggregate_version>0), source_global_sequence BIGINT NOT NULL CHECK(source_global_sequence>0),
    tenant_scope JSONB NOT NULL, scope_key TEXT NOT NULL, customer_account_id UUID NOT NULL, title TEXT NOT NULL, record_status TEXT NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL, PRIMARY KEY(tenant_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY(tenant_id,projection_name,generation) REFERENCES erpx_cqrs_read.projection_generations(tenant_id,projection_name,generation) ON DELETE CASCADE
);
CREATE TABLE erpx_support_read.platform_case_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(), projection_name TEXT NOT NULL DEFAULT 'support.platform-cases',
    generation UUID NOT NULL, partition_key TEXT NOT NULL, case_id UUID NOT NULL, source_aggregate_version BIGINT NOT NULL CHECK(source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK(source_global_sequence>0), tenant_scope JSONB NOT NULL, scope_key TEXT NOT NULL,
    case_code TEXT NOT NULL, title TEXT NOT NULL, case_status TEXT NOT NULL, projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY(tenant_id,projection_name,generation,partition_key,case_id), FOREIGN KEY(tenant_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations(tenant_id,projection_name,generation) ON DELETE CASCADE
);
CREATE TABLE erpx_people_payroll_read.time_record_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(), projection_name TEXT NOT NULL DEFAULT 'people_payroll.time-records',
    generation UUID NOT NULL, partition_key TEXT NOT NULL, time_record_id UUID NOT NULL, source_aggregate_version BIGINT NOT NULL CHECK(source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK(source_global_sequence>0), tenant_scope JSONB NOT NULL, scope_key TEXT NOT NULL,
    employee_id UUID NOT NULL, work_date DATE NOT NULL, minutes INTEGER NOT NULL, cost_center TEXT NOT NULL, time_status TEXT NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL, PRIMARY KEY(tenant_id,projection_name,generation,partition_key,time_record_id),
    FOREIGN KEY(tenant_id,projection_name,generation) REFERENCES erpx_cqrs_read.projection_generations(tenant_id,projection_name,generation) ON DELETE CASCADE
);
CREATE TABLE erpx_legal_read.legal_case_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(), projection_name TEXT NOT NULL DEFAULT 'legal.cases',
    generation UUID NOT NULL, partition_key TEXT NOT NULL, case_id UUID NOT NULL, source_aggregate_version BIGINT NOT NULL CHECK(source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK(source_global_sequence>0), tenant_scope JSONB NOT NULL, scope_key TEXT NOT NULL,
    case_reference TEXT NOT NULL, title TEXT NOT NULL, case_status TEXT NOT NULL, retention_until TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL, PRIMARY KEY(tenant_id,projection_name,generation,partition_key,case_id),
    FOREIGN KEY(tenant_id,projection_name,generation) REFERENCES erpx_cqrs_read.projection_generations(tenant_id,projection_name,generation) ON DELETE CASCADE
);
CREATE TABLE erpx_shared_read.signature_envelope_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(), projection_name TEXT NOT NULL DEFAULT 'shared.signatures',
    generation UUID NOT NULL, partition_key TEXT NOT NULL, envelope_id UUID NOT NULL, source_aggregate_version BIGINT NOT NULL CHECK(source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK(source_global_sequence>0), tenant_scope JSONB NOT NULL, scope_key TEXT NOT NULL,
    document_id UUID NOT NULL, document_version BIGINT NOT NULL, envelope_status TEXT NOT NULL, retention_until TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL, PRIMARY KEY(tenant_id,projection_name,generation,partition_key,envelope_id),
    FOREIGN KEY(tenant_id,projection_name,generation) REFERENCES erpx_cqrs_read.projection_generations(tenant_id,projection_name,generation) ON DELETE CASCADE
);

DO $$ DECLARE item RECORD; BEGIN
    FOR item IN SELECT * FROM (VALUES
        ('erpx_customer_authority_read','business_authority_projection','customer_authority_completion_read_isolation'),
        ('erpx_customer_360_read','owned_record_projection','customer_360_owned_read_isolation'),
        ('erpx_support_read','platform_case_projection','support_platform_case_read_isolation'),
        ('erpx_people_payroll_read','time_record_projection','people_time_record_read_isolation'),
        ('erpx_legal_read','legal_case_projection','legal_case_read_isolation'),
        ('erpx_shared_read','signature_envelope_projection','signature_envelope_read_isolation')) AS v(schema_name,table_name,policy_name)
    LOOP
        EXECUTE format('ALTER TABLE %I.%I ENABLE ROW LEVEL SECURITY',item.schema_name,item.table_name);
        EXECUTE format('ALTER TABLE %I.%I FORCE ROW LEVEL SECURITY',item.schema_name,item.table_name);
        EXECUTE format('CREATE POLICY %I ON %I.%I USING (tenant_id=erpx_cqrs_read.current_tenant_id()) WITH CHECK (tenant_id=erpx_cqrs_read.current_tenant_id())',item.policy_name,item.schema_name,item.table_name);
    END LOOP;
END $$;

-- D13 Batch H: rebuildable Assets lifecycle-record projection.
CREATE TABLE erpx_assets_read.business_record_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'asset.lifecycle-records'
        CHECK (projection_name='asset.lifecycle-records'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (record_type IN (
        'asset.transfer','asset.depreciation-run','asset.disposal'
    )),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence>0),
    tenant_scope JSONB NOT NULL,
    record_title TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY (tenant_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (tenant_id,projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX assets_business_record_projection_scope
    ON erpx_assets_read.business_record_projection
        (tenant_id,generation,record_type,lifecycle_state,record_id);
ALTER TABLE erpx_assets_read.business_record_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_assets_read.business_record_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY assets_business_record_read_tenant_isolation
    ON erpx_assets_read.business_record_projection
    USING (tenant_id=erpx_cqrs_read.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_read.current_tenant_id());

-- D13 Batch I: rebuildable Maintenance lifecycle-record projection.
CREATE TABLE erpx_maintenance_read.business_record_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'maintenance.lifecycle-records'
        CHECK (projection_name='maintenance.lifecycle-records'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (record_type IN (
        'maintenance.plan','maintenance.inspection','maintenance.request'
    )),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence>0),
    tenant_scope JSONB NOT NULL,
    record_title TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY (tenant_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (tenant_id,projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX maintenance_business_record_projection_scope
    ON erpx_maintenance_read.business_record_projection
        (tenant_id,generation,record_type,lifecycle_state,record_id);
ALTER TABLE erpx_maintenance_read.business_record_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_maintenance_read.business_record_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY maintenance_business_record_read_tenant_isolation
    ON erpx_maintenance_read.business_record_projection
    USING (tenant_id=erpx_cqrs_read.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_read.current_tenant_id());

-- D13 Batch J: rebuildable Quality lifecycle-record projection.
CREATE TABLE erpx_quality_read.business_record_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'quality.lifecycle-records'
        CHECK (projection_name='quality.lifecycle-records'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (record_type IN (
        'quality.plan','quality.nonconformance','quality.corrective-action'
    )),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence>0),
    tenant_scope JSONB NOT NULL,
    record_title TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY (tenant_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (tenant_id,projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX quality_business_record_projection_scope
    ON erpx_quality_read.business_record_projection
        (tenant_id,generation,record_type,lifecycle_state,record_id);
ALTER TABLE erpx_quality_read.business_record_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_quality_read.business_record_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY quality_business_record_read_tenant_isolation
    ON erpx_quality_read.business_record_projection
    USING (tenant_id=erpx_cqrs_read.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_read.current_tenant_id());

-- D13 Batch K: rebuildable Operations lifecycle-record projection.
CREATE TABLE erpx_operations_read.business_record_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'operations.lifecycle-records'
        CHECK (projection_name='operations.lifecycle-records'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (record_type IN (
        'operations.operation','operations.case','operations.approval-work'
    )),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence>0),
    tenant_scope JSONB NOT NULL,
    record_title TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY (tenant_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (tenant_id,projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX operations_business_record_projection_scope
    ON erpx_operations_read.business_record_projection
        (tenant_id,generation,record_type,lifecycle_state,record_id);
ALTER TABLE erpx_operations_read.business_record_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_operations_read.business_record_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY operations_business_record_read_tenant_isolation
    ON erpx_operations_read.business_record_projection
    USING (tenant_id=erpx_cqrs_read.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_read.current_tenant_id());

-- D13 Batch L: rebuildable Commercial lifecycle-record projection.
CREATE TABLE erpx_commercial_read.business_record_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'commercial.lifecycle-records'
        CHECK (projection_name='commercial.lifecycle-records'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (record_type IN (
        'commercial.account','commercial.adjustment'
    )),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence>0),
    tenant_scope JSONB NOT NULL,
    record_title TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY (tenant_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (tenant_id,projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX commercial_business_record_projection_scope
    ON erpx_commercial_read.business_record_projection
        (tenant_id,generation,record_type,lifecycle_state,record_id);
ALTER TABLE erpx_commercial_read.business_record_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_commercial_read.business_record_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY commercial_business_record_read_tenant_isolation
    ON erpx_commercial_read.business_record_projection
    USING (tenant_id=erpx_cqrs_read.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_read.current_tenant_id());

-- D13 Batch M: rebuildable POS lifecycle-record projection.
CREATE TABLE erpx_pos_read.business_record_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'pos.lifecycle-records'
        CHECK (projection_name='pos.lifecycle-records'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (record_type IN (
        'pos.register-session','pos.cash-movement'
    )),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence>0),
    tenant_scope JSONB NOT NULL,
    record_title TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY (tenant_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (tenant_id,projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX pos_business_record_projection_scope
    ON erpx_pos_read.business_record_projection
        (tenant_id,generation,record_type,lifecycle_state,record_id);
ALTER TABLE erpx_pos_read.business_record_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_pos_read.business_record_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY pos_business_record_read_tenant_isolation
    ON erpx_pos_read.business_record_projection
    USING (tenant_id=erpx_cqrs_read.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_read.current_tenant_id());

-- D13 Batch N: rebuildable Sales lifecycle-record projection.
CREATE TABLE erpx_sales_read.business_record_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'sales.lifecycle-records'
        CHECK (projection_name='sales.lifecycle-records'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (record_type IN (
        'sales.return','sales.fulfillment-handoff'
    )),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence>0),
    tenant_scope JSONB NOT NULL,
    record_title TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY (tenant_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (tenant_id,projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX sales_business_record_projection_scope
    ON erpx_sales_read.business_record_projection
        (tenant_id,generation,record_type,lifecycle_state,record_id);
ALTER TABLE erpx_sales_read.business_record_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_sales_read.business_record_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY sales_business_record_read_tenant_isolation
    ON erpx_sales_read.business_record_projection
    USING (tenant_id=erpx_cqrs_read.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_read.current_tenant_id());

-- D13 Batch O: rebuildable Inventory lifecycle-record projection.
CREATE TABLE erpx_inventory_read.business_record_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'inventory.lifecycle-records'
        CHECK (projection_name='inventory.lifecycle-records'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (record_type IN (
        'inventory.document','inventory.transfer','inventory.count','inventory.lot','inventory.serial'
    )),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence>0),
    tenant_scope JSONB NOT NULL,
    record_title TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY (tenant_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (tenant_id,projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX inventory_business_record_projection_scope
    ON erpx_inventory_read.business_record_projection
        (tenant_id,generation,record_type,lifecycle_state,record_id);
ALTER TABLE erpx_inventory_read.business_record_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_inventory_read.business_record_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY inventory_business_record_read_tenant_isolation
    ON erpx_inventory_read.business_record_projection
    USING (tenant_id=erpx_cqrs_read.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_read.current_tenant_id());

-- Final generation-two role marker: erpx_meta.schema_manifest,
-- D13 Batch O2: active-generation reservation projection derived from StockItem snapshots.
CREATE TABLE erpx_inventory_read.reservation_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'inventory.stock_items'
        CHECK (projection_name='inventory.stock_items'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    reservation_id UUID NOT NULL,
    stock_item_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    warehouse_id UUID NOT NULL,
    item_code TEXT NOT NULL,
    quantity BIGINT NOT NULL CHECK (quantity > 0),
    demand_reference TEXT NOT NULL,
    reservation_status TEXT NOT NULL CHECK (reservation_status IN ('active','released')),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,projection_name,generation,partition_key,reservation_id),
    FOREIGN KEY (tenant_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (tenant_id,projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX inventory_reservation_projection_scope
    ON erpx_inventory_read.reservation_projection
        (tenant_id,generation,stock_item_id,reservation_status,reservation_id);
ALTER TABLE erpx_inventory_read.reservation_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_inventory_read.reservation_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY inventory_reservation_read_tenant_isolation
    ON erpx_inventory_read.reservation_projection
    USING (tenant_id=erpx_cqrs_read.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_read.current_tenant_id());

-- Final generation-two role marker: erpx_meta.schema_manifest,
-- schema_generation=2, database_role=tenant_read, migration_set=tenant-read-v2.
UPDATE erpx_meta.schema_manifest
SET installed_at = now()
WHERE singleton AND schema_generation = 2
  AND database_role = 'tenant_read' AND migration_set = 'tenant-read-v2';
