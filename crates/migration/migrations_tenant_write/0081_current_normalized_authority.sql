-- Typed relational authority for current-context aggregates migrated away
-- from the shared aggregate snapshot store. Additional context sections may
-- be appended here before the final generation marker.

CREATE TABLE erpx_quality_write.inspection_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    inspection_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    reference TEXT NOT NULL CHECK (octet_length(reference) BETWEEN 1 AND 255),
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    inspection_kind TEXT NOT NULL CHECK (inspection_kind IN (
        'incoming','in_process','final'
    )),
    product_id UUID NOT NULL,
    lot_reference TEXT CHECK (
        lot_reference IS NULL OR octet_length(lot_reference) BETWEEN 1 AND 255
    ),
    quantity BIGINT NOT NULL CHECK (quantity > 0),
    passed_quantity BIGINT NOT NULL CHECK (passed_quantity >= 0),
    failed_quantity BIGINT NOT NULL CHECK (failed_quantity >= 0),
    inspection_status TEXT NOT NULL CHECK (inspection_status IN (
        'draft','in_progress','passed','failed','released','cancelled'
    )),
    disposition_note TEXT CHECK (
        disposition_note IS NULL OR octet_length(disposition_note) BETWEEN 1 AND 255
    ),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, inspection_id),
    UNIQUE (tenant_id, inspection_id, aggregate_version),
    UNIQUE NULLS NOT DISTINCT (
        tenant_id, holding_id, company_id, branch_id, reference
    ),
    CHECK (passed_quantity + failed_quantity <= quantity),
    CHECK (updated_at >= created_at),
    CHECK (
        (inspection_status IN ('draft','in_progress')
            AND passed_quantity=0 AND failed_quantity=0)
        OR (inspection_status IN ('passed','released')
            AND passed_quantity=quantity AND failed_quantity=0)
        OR (inspection_status='failed'
            AND failed_quantity>0
            AND passed_quantity + failed_quantity=quantity
            AND disposition_note IS NOT NULL)
        OR inspection_status='cancelled'
    )
);

CREATE INDEX quality_inspection_authority_scope
    ON erpx_quality_write.inspection_authority
        (tenant_id, holding_id, company_id, branch_id, inspection_status,
         updated_at DESC, inspection_id DESC);

ALTER TABLE erpx_quality_write.inspection_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_quality_write.inspection_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY quality_inspection_authority_tenant_isolation
    ON erpx_quality_write.inspection_authority
    USING (tenant_id=erpx_cqrs_write.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id());

-- Organization hierarchy authority. This replaces the generation-one
-- hierarchy guard plus shared aggregate snapshot combination with one typed,
-- tenant-composite authoritative relation.
CREATE TABLE erpx_organization_write.organization_node_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    node_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    node_kind TEXT NOT NULL CHECK (node_kind IN (
        'holding','company','headquarters','branch','department','team'
    )),
    parent_node_id UUID,
    lineage_node_ids UUID[] NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID,
    branch_id UUID,
    department_id UUID,
    node_code TEXT NOT NULL CHECK (octet_length(node_code) BETWEEN 1 AND 128),
    node_name TEXT NOT NULL CHECK (octet_length(node_name) BETWEEN 1 AND 256),
    node_status TEXT NOT NULL CHECK (node_status IN ('active','suspended','archived')),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,node_id),
    UNIQUE (tenant_id,node_id,aggregate_version),
    FOREIGN KEY (tenant_id,parent_node_id)
        REFERENCES erpx_organization_write.organization_node_authority(tenant_id,node_id)
        DEFERRABLE INITIALLY DEFERRED,
    CHECK (NOT node_id=ANY(lineage_node_ids)),
    CHECK (updated_at >= created_at),
    CHECK (
        (node_kind='holding' AND parent_node_id IS NULL
            AND holding_id=node_id AND company_id IS NULL
            AND branch_id IS NULL AND department_id IS NULL)
        OR (node_kind='company' AND parent_node_id=holding_id
            AND company_id=node_id AND branch_id IS NULL AND department_id IS NULL)
        OR (node_kind='headquarters' AND parent_node_id=company_id
            AND company_id IS NOT NULL AND branch_id IS NULL AND department_id IS NULL)
        OR (node_kind='branch' AND parent_node_id=company_id
            AND company_id IS NOT NULL AND branch_id=node_id AND department_id IS NULL)
        OR (node_kind='department' AND parent_node_id=COALESCE(branch_id,company_id)
            AND company_id IS NOT NULL AND department_id=node_id)
        OR (node_kind='team' AND parent_node_id=department_id
            AND company_id IS NOT NULL AND department_id IS NOT NULL)
    )
);

CREATE UNIQUE INDEX organization_node_authority_sibling_code
    ON erpx_organization_write.organization_node_authority
        (tenant_id,holding_id,COALESCE(parent_node_id,holding_id),node_code)
    WHERE node_status <> 'archived';
CREATE INDEX organization_node_authority_children
    ON erpx_organization_write.organization_node_authority
        (tenant_id,parent_node_id,node_status,node_id);
CREATE INDEX organization_node_authority_lineage
    ON erpx_organization_write.organization_node_authority USING GIN (lineage_node_ids);

ALTER TABLE erpx_organization_write.organization_node_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_organization_write.organization_node_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY organization_node_authority_tenant_isolation
    ON erpx_organization_write.organization_node_authority
    USING (tenant_id=erpx_cqrs_write.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id());

-- Assets write authority: scalar asset identity, scope and lifecycle state.
CREATE TABLE erpx_assets_write.asset_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    asset_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    asset_code TEXT NOT NULL CHECK (octet_length(asset_code) BETWEEN 1 AND 128),
    asset_name TEXT NOT NULL CHECK (octet_length(asset_name) BETWEEN 1 AND 256),
    location_id UUID NOT NULL,
    asset_status TEXT NOT NULL CHECK (asset_status IN (
        'draft','in_service','under_maintenance','retired'
    )),
    asset_condition TEXT NOT NULL CHECK (asset_condition IN (
        'operational','degraded','out_of_service','retired'
    )),
    placed_in_service_at TIMESTAMPTZ,
    retired_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,asset_id),
    UNIQUE (tenant_id,asset_id,aggregate_version),
    UNIQUE (tenant_id,scope_key,asset_code),
    CHECK (updated_at >= created_at),
    CHECK (placed_in_service_at IS NULL OR placed_in_service_at BETWEEN created_at AND updated_at),
    CHECK (retired_at IS NULL OR retired_at BETWEEN created_at AND updated_at),
    CHECK (retired_at IS NULL OR placed_in_service_at IS NULL OR retired_at >= placed_in_service_at),
    CHECK (
        (asset_status='draft' AND asset_condition='out_of_service'
            AND placed_in_service_at IS NULL AND retired_at IS NULL)
        OR (asset_status='in_service' AND asset_condition IN ('operational','degraded')
            AND placed_in_service_at IS NOT NULL AND retired_at IS NULL)
        OR (asset_status='under_maintenance' AND asset_condition='out_of_service'
            AND placed_in_service_at IS NOT NULL AND retired_at IS NULL)
        OR (asset_status='retired' AND asset_condition='retired' AND retired_at IS NOT NULL)
    )
);

CREATE INDEX asset_authority_scope_location_status
    ON erpx_assets_write.asset_authority
        (tenant_id,holding_id,company_id,branch_id,location_id,asset_status,updated_at DESC);
ALTER TABLE erpx_assets_write.asset_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_assets_write.asset_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY asset_authority_tenant_isolation ON erpx_assets_write.asset_authority
    USING (tenant_id=erpx_cqrs_write.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id());

-- Procurement purchase-order authority with relational line ownership.
CREATE TABLE erpx_procurement_write.purchase_order_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    purchase_order_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    order_code TEXT NOT NULL CHECK (octet_length(order_code) BETWEEN 1 AND 128),
    supplier_id UUID NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    order_status TEXT NOT NULL CHECK (order_status IN ('draft','released','cancelled')),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,purchase_order_id),
    UNIQUE (tenant_id,purchase_order_id,aggregate_version),
    UNIQUE (tenant_id,scope_key,order_code),
    CHECK (updated_at >= created_at)
);

CREATE TABLE erpx_procurement_write.purchase_order_line_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    purchase_order_id UUID NOT NULL,
    line_id UUID NOT NULL,
    line_position INTEGER NOT NULL CHECK (line_position >= 0 AND line_position < 10000),
    item_code TEXT NOT NULL CHECK (octet_length(item_code) BETWEEN 1 AND 128),
    ordered_quantity BIGINT NOT NULL CHECK (ordered_quantity > 0),
    unit_price_minor BIGINT NOT NULL CHECK (unit_price_minor > 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    destination_code TEXT NOT NULL CHECK (octet_length(destination_code) BETWEEN 1 AND 128),
    PRIMARY KEY (tenant_id,purchase_order_id,line_id),
    UNIQUE (tenant_id,purchase_order_id,line_position),
    FOREIGN KEY (tenant_id,purchase_order_id)
        REFERENCES erpx_procurement_write.purchase_order_authority(tenant_id,purchase_order_id)
        ON DELETE CASCADE
);

CREATE INDEX purchase_order_authority_scope_supplier_status
    ON erpx_procurement_write.purchase_order_authority
        (tenant_id,holding_id,company_id,branch_id,supplier_id,order_status,updated_at DESC);
ALTER TABLE erpx_procurement_write.purchase_order_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_procurement_write.purchase_order_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY purchase_order_authority_tenant_isolation
    ON erpx_procurement_write.purchase_order_authority
    USING (tenant_id=erpx_cqrs_write.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id());
ALTER TABLE erpx_procurement_write.purchase_order_line_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_procurement_write.purchase_order_line_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY purchase_order_line_authority_tenant_isolation
    ON erpx_procurement_write.purchase_order_line_authority
    USING (tenant_id=erpx_cqrs_write.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id());

-- Inventory stock authority and child movement/reservation ledgers.
CREATE TABLE erpx_inventory_write.stock_item_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    stock_item_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    warehouse_id UUID NOT NULL,
    item_code TEXT NOT NULL CHECK (octet_length(item_code) BETWEEN 1 AND 128),
    on_hand BIGINT NOT NULL CHECK (on_hand >= 0),
    reserved BIGINT NOT NULL CHECK (reserved >= 0 AND reserved <= on_hand),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,stock_item_id),
    UNIQUE (tenant_id,stock_item_id,aggregate_version),
    UNIQUE (tenant_id,scope_key,warehouse_id,item_code),
    CHECK (updated_at >= created_at)
);
CREATE TABLE erpx_inventory_write.movement_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    movement_id UUID NOT NULL,
    stock_item_id UUID NOT NULL,
    quantity_delta BIGINT NOT NULL CHECK (quantity_delta <> 0),
    procurement_line_id UUID,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,movement_id),
    UNIQUE (tenant_id,stock_item_id,aggregate_version,movement_id),
    FOREIGN KEY (tenant_id,stock_item_id)
        REFERENCES erpx_inventory_write.stock_item_authority(tenant_id,stock_item_id)
        ON DELETE RESTRICT
);
CREATE TABLE erpx_inventory_write.reservation_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    reservation_id UUID NOT NULL,
    stock_item_id UUID NOT NULL,
    quantity BIGINT NOT NULL CHECK (quantity > 0),
    demand_reference TEXT NOT NULL CHECK (octet_length(demand_reference) BETWEEN 1 AND 128),
    reservation_status TEXT NOT NULL CHECK (reservation_status IN ('active','released')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,reservation_id),
    UNIQUE (tenant_id,stock_item_id,reservation_id),
    FOREIGN KEY (tenant_id,stock_item_id)
        REFERENCES erpx_inventory_write.stock_item_authority(tenant_id,stock_item_id)
        ON DELETE CASCADE
);
CREATE INDEX stock_item_authority_scope_warehouse_code
    ON erpx_inventory_write.stock_item_authority
        (tenant_id,holding_id,company_id,branch_id,warehouse_id,item_code);
CREATE INDEX reservation_authority_active
    ON erpx_inventory_write.reservation_authority(tenant_id,stock_item_id,reservation_id)
    WHERE reservation_status='active';
ALTER TABLE erpx_inventory_write.stock_item_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_inventory_write.stock_item_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY stock_item_authority_tenant_isolation
    ON erpx_inventory_write.stock_item_authority
    USING (tenant_id=erpx_cqrs_write.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id());
ALTER TABLE erpx_inventory_write.movement_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_inventory_write.movement_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY movement_authority_tenant_isolation ON erpx_inventory_write.movement_authority
    USING (tenant_id=erpx_cqrs_write.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id());
ALTER TABLE erpx_inventory_write.reservation_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_inventory_write.reservation_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY reservation_authority_tenant_isolation
    ON erpx_inventory_write.reservation_authority
    USING (tenant_id=erpx_cqrs_write.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id());

-- CRM accepted-lead and opportunity authority, including exact Marketing
-- provenance and complete forecast state.
CREATE TABLE erpx_crm_write.lead_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    lead_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    marketing_candidate_id UUID NOT NULL,
    campaign_id UUID NOT NULL,
    audience_member_id UUID NOT NULL,
    marketing_source_version BIGINT NOT NULL CHECK (marketing_source_version > 0),
    customer_id UUID,
    source TEXT NOT NULL CHECK (octet_length(source) BETWEEN 1 AND 255),
    qualification_score_basis_points INTEGER NOT NULL
        CHECK (qualification_score_basis_points BETWEEN 1 AND 10000),
    accepted_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,lead_id),
    UNIQUE (tenant_id,lead_id,aggregate_version),
    UNIQUE (tenant_id,marketing_candidate_id)
);
CREATE TABLE erpx_crm_write.opportunity_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    opportunity_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    accepted_lead_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    opportunity_name TEXT NOT NULL CHECK (octet_length(opportunity_name) BETWEEN 1 AND 255),
    expected_value_minor BIGINT NOT NULL CHECK (expected_value_minor > 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    opportunity_stage TEXT NOT NULL CHECK (opportunity_stage IN (
        'qualified','discovery','proposal','negotiation','won','lost'
    )),
    account_id UUID,
    owner_ref TEXT CHECK (owner_ref IS NULL OR octet_length(owner_ref) BETWEEN 1 AND 255),
    pipeline_category TEXT CHECK (pipeline_category IN ('pipeline','best_case','commit','closed')),
    probability_basis_points INTEGER CHECK (probability_basis_points BETWEEN 0 AND 10000),
    expected_close_on DATE,
    originating_campaign_id UUID,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,opportunity_id),
    UNIQUE (tenant_id,opportunity_id,aggregate_version),
    FOREIGN KEY (tenant_id,accepted_lead_id)
        REFERENCES erpx_crm_write.lead_authority(tenant_id,lead_id) ON DELETE RESTRICT,
    CHECK (updated_at >= created_at),
    CHECK (
        (account_id IS NULL AND owner_ref IS NULL AND pipeline_category IS NULL
            AND probability_basis_points IS NULL AND expected_close_on IS NULL)
        OR (account_id IS NOT NULL AND owner_ref IS NOT NULL AND pipeline_category IS NOT NULL
            AND probability_basis_points IS NOT NULL AND expected_close_on IS NOT NULL)
    )
);
CREATE INDEX crm_lead_authority_scope
    ON erpx_crm_write.lead_authority(tenant_id,holding_id,company_id,branch_id,accepted_at DESC);
CREATE INDEX crm_opportunity_authority_scope_stage
    ON erpx_crm_write.opportunity_authority
        (tenant_id,holding_id,company_id,branch_id,customer_id,opportunity_stage,updated_at DESC);
ALTER TABLE erpx_crm_write.lead_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_crm_write.lead_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY crm_lead_authority_tenant_isolation ON erpx_crm_write.lead_authority
    USING (tenant_id=erpx_cqrs_write.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id());
ALTER TABLE erpx_crm_write.opportunity_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_crm_write.opportunity_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY crm_opportunity_authority_tenant_isolation
    ON erpx_crm_write.opportunity_authority
    USING (tenant_id=erpx_cqrs_write.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id());

-- Marketing lead and campaign-budget authority. Dimensions, allocations and
-- spend reservations are typed columns/child rows; no aggregate document is
-- authoritative.
CREATE TABLE erpx_marketing_write.lead_candidate_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    lead_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    campaign_id UUID NOT NULL,
    audience_member_id UUID NOT NULL,
    source TEXT NOT NULL CHECK (octet_length(source) BETWEEN 1 AND 255),
    existing_customer_id UUID,
    qualification_score_basis_points INTEGER CHECK (
        qualification_score_basis_points BETWEEN 1 AND 10000
    ),
    lead_status TEXT NOT NULL CHECK (lead_status IN ('open','qualified','rejected')),
    engagement_count BIGINT NOT NULL CHECK (engagement_count >= 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,lead_id),
    UNIQUE (tenant_id,lead_id,aggregate_version),
    CHECK (updated_at >= created_at),
    CHECK ((lead_status='qualified') = (qualification_score_basis_points IS NOT NULL))
);
CREATE INDEX marketing_lead_authority_scope
    ON erpx_marketing_write.lead_candidate_authority
       (tenant_id,holding_id,company_id,branch_id,lead_status,updated_at DESC);

CREATE TABLE erpx_marketing_write.campaign_budget_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    budget_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    campaign_id UUID NOT NULL,
    budget_code TEXT NOT NULL CHECK (octet_length(budget_code) BETWEEN 1 AND 255),
    budget_name TEXT NOT NULL CHECK (octet_length(budget_name) BETWEEN 1 AND 255),
    budget_period TEXT NOT NULL CHECK (budget_period IN ('annual','quarterly','campaign')),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    requested_minor BIGINT NOT NULL CHECK (requested_minor > 0),
    approved_minor BIGINT CHECK (approved_minor > 0),
    dimension_business_unit_id UUID,
    dimension_cost_center_id UUID,
    dimension_location_id UUID,
    dimension_project_id UUID,
    dimension_channel_id UUID,
    dimension_party_id UUID,
    dimension_employee_id UUID,
    dimension_asset_id UUID,
    dimension_legal_matter_id UUID,
    dimension_intercompany_partner_id UUID,
    approval_request_id UUID,
    approval_subject_version BIGINT CHECK (approval_subject_version > 0),
    approval_requested_by TEXT CHECK (
        approval_requested_by IS NULL OR octet_length(approval_requested_by) BETWEEN 1 AND 255
    ),
    approval_requested_at TIMESTAMPTZ,
    budget_status TEXT NOT NULL CHECK (
        budget_status IN ('draft','review','approved','running','completed','cancelled')
    ),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,budget_id),
    UNIQUE (tenant_id,budget_id,aggregate_version),
    UNIQUE (tenant_id,holding_id,company_id,branch_id,budget_code),
    CHECK (updated_at >= created_at),
    CHECK (approved_minor IS NULL OR approved_minor = requested_minor),
    CHECK (
        (approval_request_id IS NULL AND approval_subject_version IS NULL
            AND approval_requested_by IS NULL AND approval_requested_at IS NULL)
        OR (approval_request_id IS NOT NULL AND approval_subject_version IS NOT NULL
            AND approval_requested_by IS NOT NULL AND approval_requested_at IS NOT NULL)
    )
);
CREATE INDEX marketing_budget_authority_scope
    ON erpx_marketing_write.campaign_budget_authority
       (tenant_id,holding_id,company_id,branch_id,campaign_id,budget_status,updated_at DESC);

CREATE TABLE erpx_marketing_write.budget_allocation_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    budget_id UUID NOT NULL,
    allocation_id UUID NOT NULL,
    tactic_id UUID NOT NULL,
    channel_id UUID NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    planned_minor BIGINT NOT NULL CHECK (planned_minor > 0),
    budget_aggregate_version BIGINT NOT NULL CHECK (budget_aggregate_version > 0),
    PRIMARY KEY (tenant_id,budget_id,allocation_id),
    UNIQUE (tenant_id,budget_id,allocation_id,budget_aggregate_version),
    FOREIGN KEY (tenant_id,budget_id)
        REFERENCES erpx_marketing_write.campaign_budget_authority(tenant_id,budget_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_marketing_write.spend_reservation_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    request_id UUID NOT NULL,
    budget_id UUID NOT NULL,
    allocation_id UUID NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    amount_minor BIGINT NOT NULL CHECK (amount_minor > 0),
    expected_outcome TEXT NOT NULL CHECK (octet_length(expected_outcome) BETWEEN 1 AND 255),
    spend_status TEXT NOT NULL CHECK (spend_status IN ('active','released')),
    requested_at TIMESTAMPTZ NOT NULL,
    released_at TIMESTAMPTZ,
    budget_aggregate_version BIGINT NOT NULL CHECK (budget_aggregate_version > 0),
    PRIMARY KEY (tenant_id,request_id),
    UNIQUE (tenant_id,budget_id,request_id),
    FOREIGN KEY (tenant_id,budget_id,allocation_id)
        REFERENCES erpx_marketing_write.budget_allocation_authority
            (tenant_id,budget_id,allocation_id) ON DELETE RESTRICT,
    CHECK (
        (spend_status='active' AND released_at IS NULL)
        OR (spend_status='released' AND released_at >= requested_at)
    )
);

ALTER TABLE erpx_marketing_write.lead_candidate_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_marketing_write.lead_candidate_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY marketing_lead_candidate_authority_tenant_isolation
    ON erpx_marketing_write.lead_candidate_authority
    USING (tenant_id=erpx_cqrs_write.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id());
ALTER TABLE erpx_marketing_write.campaign_budget_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_marketing_write.campaign_budget_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY marketing_campaign_budget_authority_tenant_isolation
    ON erpx_marketing_write.campaign_budget_authority
    USING (tenant_id=erpx_cqrs_write.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id());
ALTER TABLE erpx_marketing_write.budget_allocation_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_marketing_write.budget_allocation_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY marketing_budget_allocation_authority_tenant_isolation
    ON erpx_marketing_write.budget_allocation_authority
    USING (tenant_id=erpx_cqrs_write.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id());
ALTER TABLE erpx_marketing_write.spend_reservation_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_marketing_write.spend_reservation_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY marketing_spend_reservation_authority_tenant_isolation
    ON erpx_marketing_write.spend_reservation_authority
    USING (tenant_id=erpx_cqrs_write.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id());

-- Commercial tenant-plane subscription and entitlement authority. Catalog
-- plans remain in the separately normalized control-plane authority.
CREATE TABLE erpx_commercial_write.subscription_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    subscription_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    holding_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    catalog_plan_id UUID NOT NULL,
    subscription_status TEXT NOT NULL CHECK (
        subscription_status IN ('trial','active','past_due','suspended','ended')
    ),
    amount_minor BIGINT NOT NULL CHECK (amount_minor > 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    last_provider_event_id TEXT CHECK (
        last_provider_event_id IS NULL OR octet_length(last_provider_event_id) BETWEEN 1 AND 255
    ),
    last_provider_event_at TIMESTAMPTZ,
    grace_expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,subscription_id),
    UNIQUE (tenant_id,subscription_id,aggregate_version),
    CHECK (updated_at >= created_at),
    CHECK ((last_provider_event_id IS NULL) = (last_provider_event_at IS NULL)),
    CHECK ((subscription_status='past_due') = (grace_expires_at IS NOT NULL)),
    CHECK (last_provider_event_at IS NULL OR last_provider_event_at <= updated_at)
);
CREATE INDEX commercial_subscription_authority_scope
    ON erpx_commercial_write.subscription_authority
       (tenant_id,holding_id,customer_id,subscription_status,updated_at DESC);

CREATE TABLE erpx_commercial_write.entitlement_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    entitlement_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    holding_id UUID NOT NULL,
    subscription_id UUID NOT NULL,
    capability TEXT NOT NULL CHECK (octet_length(capability) BETWEEN 1 AND 255),
    enabled BOOLEAN NOT NULL,
    seat_capacity BIGINT NOT NULL CHECK (seat_capacity > 0),
    company_capacity BIGINT NOT NULL CHECK (company_capacity > 0),
    branch_capacity BIGINT NOT NULL CHECK (branch_capacity > 0),
    allocated_seats BIGINT NOT NULL CHECK (
        allocated_seats >= 0 AND allocated_seats <= seat_capacity
    ),
    allocated_companies BIGINT NOT NULL CHECK (
        allocated_companies >= 0 AND allocated_companies <= company_capacity
    ),
    allocated_branches BIGINT NOT NULL CHECK (
        allocated_branches >= 0 AND allocated_branches <= branch_capacity
    ),
    last_provider_event_id TEXT CHECK (
        last_provider_event_id IS NULL OR octet_length(last_provider_event_id) BETWEEN 1 AND 255
    ),
    last_provider_event_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,entitlement_id),
    UNIQUE (tenant_id,entitlement_id,aggregate_version),
    UNIQUE (tenant_id,subscription_id,capability),
    FOREIGN KEY (tenant_id,subscription_id)
        REFERENCES erpx_commercial_write.subscription_authority(tenant_id,subscription_id)
        ON DELETE RESTRICT,
    CHECK (updated_at >= created_at),
    CHECK ((last_provider_event_id IS NULL) = (last_provider_event_at IS NULL)),
    CHECK (last_provider_event_at IS NULL OR last_provider_event_at <= updated_at)
);
CREATE INDEX commercial_entitlement_authority_scope
    ON erpx_commercial_write.entitlement_authority
       (tenant_id,holding_id,subscription_id,enabled,updated_at DESC);

ALTER TABLE erpx_commercial_write.subscription_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_commercial_write.subscription_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY commercial_subscription_authority_tenant_isolation
    ON erpx_commercial_write.subscription_authority
    USING (tenant_id=erpx_cqrs_write.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id());
ALTER TABLE erpx_commercial_write.entitlement_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_commercial_write.entitlement_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY commercial_entitlement_authority_tenant_isolation
    ON erpx_commercial_write.entitlement_authority
    USING (tenant_id=erpx_cqrs_write.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id());

-- Maintenance work-order authority and immutable owned downtime intervals.
CREATE TABLE erpx_maintenance_write.work_order_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    work_order_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    work_order_code TEXT NOT NULL CHECK (octet_length(work_order_code) BETWEEN 1 AND 128),
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    source_event_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    asset_id UUID NOT NULL,
    request_code TEXT NOT NULL CHECK (octet_length(request_code) BETWEEN 1 AND 255),
    asset_code TEXT NOT NULL CHECK (octet_length(asset_code) BETWEEN 1 AND 255),
    asset_name TEXT NOT NULL CHECK (octet_length(asset_name) BETWEEN 1 AND 255),
    asset_location_id UUID NOT NULL,
    asset_condition TEXT NOT NULL CHECK (
        asset_condition IN ('operational','degraded','out_of_service','retired')
    ),
    title TEXT NOT NULL CHECK (octet_length(title) BETWEEN 1 AND 255),
    work_order_status TEXT NOT NULL CHECK (
        work_order_status IN ('planned','scheduled','in_progress','blocked','completed','cancelled')
    ),
    scheduled_for TIMESTAMPTZ,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,work_order_id),
    UNIQUE (tenant_id,work_order_id,aggregate_version),
    UNIQUE (tenant_id,source_event_id),
    UNIQUE (tenant_id,holding_id,company_id,branch_id,work_order_code),
    CHECK (updated_at >= created_at),
    CHECK (asset_condition <> 'retired'),
    CHECK (scheduled_for IS NULL OR scheduled_for >= created_at),
    CHECK (started_at IS NULL OR (scheduled_for IS NOT NULL AND started_at >= scheduled_for)),
    CHECK (completed_at IS NULL OR (started_at IS NOT NULL AND completed_at > started_at))
);
CREATE INDEX maintenance_work_order_authority_scope
    ON erpx_maintenance_write.work_order_authority
       (tenant_id,holding_id,company_id,branch_id,asset_id,work_order_status,updated_at DESC);

CREATE TABLE erpx_maintenance_write.downtime_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    work_order_id UUID NOT NULL,
    downtime_id UUID NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    ended_at TIMESTAMPTZ NOT NULL,
    reason TEXT NOT NULL CHECK (octet_length(reason) BETWEEN 1 AND 1024),
    work_order_aggregate_version BIGINT NOT NULL CHECK (work_order_aggregate_version > 0),
    PRIMARY KEY (tenant_id,work_order_id,downtime_id),
    UNIQUE (tenant_id,downtime_id),
    FOREIGN KEY (tenant_id,work_order_id)
        REFERENCES erpx_maintenance_write.work_order_authority(tenant_id,work_order_id)
        ON DELETE RESTRICT,
    CHECK (ended_at > started_at)
);

ALTER TABLE erpx_maintenance_write.work_order_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_maintenance_write.work_order_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY maintenance_work_order_authority_tenant_isolation
    ON erpx_maintenance_write.work_order_authority
    USING (tenant_id=erpx_cqrs_write.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id());
ALTER TABLE erpx_maintenance_write.downtime_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_maintenance_write.downtime_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY maintenance_downtime_authority_tenant_isolation
    ON erpx_maintenance_write.downtime_authority
    USING (tenant_id=erpx_cqrs_write.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id());

-- BEGIN FINANCE_SALES_POS_LEGAL_NORMALIZED_AUTHORITY
-- Context-owned command authority for the Finance, Sales, Point of Sale, and
-- Legal aggregates. JSON columns retained by earlier migrations are derived
-- compatibility evidence only; command rehydration reads these typed columns.

ALTER TABLE erpx_sales_write.quote_guards
    ADD COLUMN holding_id UUID,
    ADD COLUMN company_id UUID,
    ADD COLUMN branch_id UUID;
ALTER TABLE erpx_sales_write.sales_order_guards
    ADD COLUMN holding_id UUID,
    ADD COLUMN company_id UUID,
    ADD COLUMN branch_id UUID;
ALTER TABLE erpx_sales_write.revenue_arrangement_guards
    ADD COLUMN holding_id UUID,
    ADD COLUMN company_id UUID,
    ADD COLUMN branch_id UUID;

CREATE TABLE erpx_sales_write.quote_line_authority (
    quote_id UUID NOT NULL REFERENCES erpx_sales_write.quote_guards(quote_id) ON DELETE CASCADE,
    line_id UUID NOT NULL,
    item_code TEXT NOT NULL,
    description TEXT NOT NULL,
    quantity BIGINT NOT NULL,
    unit_price_minor BIGINT NOT NULL,
    tax_minor BIGINT NOT NULL,
    currency TEXT NOT NULL,
    PRIMARY KEY (quote_id,line_id)
);
CREATE TABLE erpx_sales_write.sales_order_line_authority (
    sales_order_id UUID NOT NULL REFERENCES erpx_sales_write.sales_order_guards(sales_order_id) ON DELETE CASCADE,
    line_id UUID NOT NULL,
    item_code TEXT NOT NULL,
    description TEXT NOT NULL,
    quantity BIGINT NOT NULL,
    unit_price_minor BIGINT NOT NULL,
    tax_minor BIGINT NOT NULL,
    currency TEXT NOT NULL,
    PRIMARY KEY (sales_order_id,line_id)
);
CREATE TABLE erpx_sales_write.arrangement_dimension_authority (
    arrangement_id UUID PRIMARY KEY REFERENCES erpx_sales_write.revenue_arrangement_guards(arrangement_id) ON DELETE CASCADE,
    legal_entity_id UUID NOT NULL,
    branch_dimension_id UUID,
    business_unit_id UUID,
    cost_center_id UUID,
    location_id UUID,
    project_id UUID,
    campaign_id UUID,
    channel_id UUID,
    party_id UUID,
    employee_id UUID,
    asset_id UUID,
    legal_matter_id UUID,
    intercompany_partner_id UUID
);
CREATE TABLE erpx_sales_write.arrangement_obligation_authority (
    arrangement_id UUID NOT NULL REFERENCES erpx_sales_write.revenue_arrangement_guards(arrangement_id) ON DELETE CASCADE,
    obligation_id UUID NOT NULL,
    line_id UUID NOT NULL,
    revenue_model TEXT NOT NULL CHECK (revenue_model IN ('goods','service_milestone','recurring')),
    allocated_minor BIGINT NOT NULL,
    allocated_tax_minor BIGINT NOT NULL,
    accepted_minor BIGINT NOT NULL,
    accepted_tax_minor BIGINT NOT NULL,
    service_starts_on DATE,
    service_ends_on DATE,
    PRIMARY KEY (arrangement_id,obligation_id),
    UNIQUE (arrangement_id,line_id)
);
CREATE TABLE erpx_sales_write.arrangement_fulfillment_authority (
    arrangement_id UUID NOT NULL REFERENCES erpx_sales_write.revenue_arrangement_guards(arrangement_id) ON DELETE CASCADE,
    fulfillment_id UUID NOT NULL,
    billing_event_id UUID NOT NULL,
    obligation_id UUID NOT NULL,
    fulfillment_kind TEXT NOT NULL CHECK (fulfillment_kind IN ('goods_delivery','service_milestone','recurring_period')),
    accepted_minor BIGINT NOT NULL,
    accepted_tax_minor BIGINT NOT NULL,
    accepted_at TIMESTAMPTZ NOT NULL,
    eligible_on DATE NOT NULL,
    PRIMARY KEY (arrangement_id,fulfillment_id),
    UNIQUE (arrangement_id,billing_event_id),
    FOREIGN KEY (arrangement_id,obligation_id)
        REFERENCES erpx_sales_write.arrangement_obligation_authority(arrangement_id,obligation_id)
);

ALTER TABLE erpx_pos_write.sale_guards
    ADD COLUMN holding_id UUID,
    ADD COLUMN company_id UUID,
    ADD COLUMN branch_id UUID;
ALTER TABLE erpx_pos_write.refund_guards
    ADD COLUMN holding_id UUID,
    ADD COLUMN company_id UUID,
    ADD COLUMN branch_id UUID;

ALTER TABLE erpx_finance_write.chart_guards ADD COLUMN holding_id UUID, ADD COLUMN company_id UUID, ADD COLUMN branch_id UUID;
ALTER TABLE erpx_finance_write.period_guards ADD COLUMN holding_id UUID, ADD COLUMN company_id UUID, ADD COLUMN branch_id UUID;
ALTER TABLE erpx_finance_write.journal_guards ADD COLUMN holding_id UUID, ADD COLUMN company_id UUID, ADD COLUMN branch_id UUID;
ALTER TABLE erpx_finance_write.subledger_guards ADD COLUMN holding_id UUID, ADD COLUMN company_id UUID, ADD COLUMN branch_id UUID;
ALTER TABLE erpx_finance_write.customer_invoice_guards ADD COLUMN holding_id UUID, ADD COLUMN company_id UUID, ADD COLUMN branch_id UUID;
ALTER TABLE erpx_finance_write.revenue_schedule_guards
    ADD COLUMN holding_id UUID,
    ADD COLUMN company_id UUID,
    ADD COLUMN branch_id UUID,
    ADD COLUMN legal_contract_id UUID,
    ADD COLUMN order_code TEXT;

CREATE TABLE erpx_finance_write.customer_invoice_dimension_authority (
    invoice_id UUID PRIMARY KEY REFERENCES erpx_finance_write.customer_invoice_guards(invoice_id) ON DELETE CASCADE,
    fulfillment_id UUID NOT NULL,
    arrangement_id UUID NOT NULL,
    sales_order_id UUID NOT NULL,
    order_code TEXT NOT NULL,
    legal_entity_id UUID NOT NULL,
    branch_dimension_id UUID,
    business_unit_id UUID,
    cost_center_id UUID,
    location_id UUID,
    project_id UUID,
    campaign_id UUID,
    channel_id UUID,
    party_id UUID,
    employee_id UUID,
    asset_id UUID,
    legal_matter_id UUID,
    intercompany_partner_id UUID
);
CREATE TABLE erpx_finance_write.revenue_schedule_dimension_authority (
    schedule_id UUID PRIMARY KEY REFERENCES erpx_finance_write.revenue_schedule_guards(schedule_id) ON DELETE CASCADE,
    legal_entity_id UUID NOT NULL,
    branch_dimension_id UUID,
    business_unit_id UUID,
    cost_center_id UUID,
    location_id UUID,
    project_id UUID,
    campaign_id UUID,
    channel_id UUID,
    party_id UUID,
    employee_id UUID,
    asset_id UUID,
    legal_matter_id UUID,
    intercompany_partner_id UUID
);
CREATE TABLE erpx_finance_write.revenue_schedule_obligation_authority (
    schedule_id UUID NOT NULL REFERENCES erpx_finance_write.revenue_schedule_guards(schedule_id) ON DELETE CASCADE,
    obligation_id UUID NOT NULL,
    line_id UUID NOT NULL,
    revenue_model TEXT NOT NULL CHECK (revenue_model IN ('goods','service_milestone','recurring')),
    allocated_minor BIGINT NOT NULL,
    recognized_minor BIGINT NOT NULL,
    service_starts_on DATE,
    service_ends_on DATE,
    PRIMARY KEY (schedule_id,obligation_id)
);
CREATE TABLE erpx_finance_write.revenue_schedule_recognition_authority (
    schedule_id UUID NOT NULL REFERENCES erpx_finance_write.revenue_schedule_guards(schedule_id) ON DELETE CASCADE,
    recognition_ordinal BIGINT NOT NULL,
    fulfillment_id UUID NOT NULL,
    obligation_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL,
    recognized_minor BIGINT NOT NULL,
    recognized_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (schedule_id,recognition_ordinal),
    UNIQUE (schedule_id,fulfillment_id),
    FOREIGN KEY (schedule_id,obligation_id)
        REFERENCES erpx_finance_write.revenue_schedule_obligation_authority(schedule_id,obligation_id)
);

CREATE TABLE erpx_finance_write.journal_source_authority (
    journal_id UUID PRIMARY KEY REFERENCES erpx_finance_write.journal_guards(journal_id) ON DELETE CASCADE,
    source_kind TEXT NOT NULL,
    sales_order_id UUID,
    sales_order_code TEXT,
    sales_customer_id UUID,
    sales_order_status TEXT,
    sales_total_minor BIGINT,
    inventory_movement_id UUID,
    inventory_stock_item_id UUID,
    inventory_warehouse_id UUID,
    inventory_item_code TEXT,
    inventory_quantity_units BIGINT,
    inventory_quantity_scale SMALLINT,
    inventory_procurement_line_id UUID,
    payroll_run_id UUID,
    payroll_period_code TEXT,
    payroll_finalized_at TIMESTAMPTZ,
    payroll_gross_minor BIGINT,
    payroll_deduction_minor BIGINT,
    payroll_net_minor BIGINT,
    pos_transaction_id UUID,
    pos_original_sale_id UUID,
    pos_register_id UUID,
    pos_receipt_code TEXT,
    pos_customer_id UUID,
    pos_amount_minor BIGINT,
    pos_occurred_at TIMESTAMPTZ,
    manual_reference TEXT
);
CREATE TABLE erpx_finance_write.journal_sales_line_authority (
    journal_id UUID NOT NULL REFERENCES erpx_finance_write.journal_source_authority(journal_id) ON DELETE CASCADE,
    line_id UUID NOT NULL,
    item_code TEXT NOT NULL,
    quantity_units BIGINT NOT NULL,
    quantity_scale SMALLINT NOT NULL,
    line_value_minor BIGINT NOT NULL,
    PRIMARY KEY (journal_id,line_id)
);

ALTER TABLE erpx_legal_write.contract_guards ADD COLUMN holding_id UUID, ADD COLUMN company_id UUID, ADD COLUMN branch_id UUID;
ALTER TABLE erpx_legal_write.document_guards ADD COLUMN holding_id UUID, ADD COLUMN company_id UUID, ADD COLUMN branch_id UUID;
ALTER TABLE erpx_legal_write.access_entry_guards ADD COLUMN holding_id UUID, ADD COLUMN company_id UUID, ADD COLUMN branch_id UUID;
ALTER TABLE erpx_legal_write.matter_guards
    ADD COLUMN holding_id UUID,
    ADD COLUMN company_id UUID,
    ADD COLUMN branch_id UUID,
    ADD COLUMN hold_active BOOLEAN,
    ADD COLUMN hold_reason TEXT,
    ADD COLUMN hold_placed_by TEXT,
    ADD COLUMN hold_placed_at TIMESTAMPTZ,
    ADD COLUMN hold_released_by TEXT,
    ADD COLUMN hold_released_at TIMESTAMPTZ;
ALTER TABLE erpx_legal_write.matter_link_guards
    ADD COLUMN target_holding_id UUID,
    ADD COLUMN target_company_id UUID,
    ADD COLUMN target_branch_id UUID;

-- Backfill exact scope keys without consulting generic aggregate snapshots.
DO $$
DECLARE relation_name TEXT;
BEGIN
    FOREACH relation_name IN ARRAY ARRAY[
      'erpx_sales_write.quote_guards','erpx_sales_write.sales_order_guards',
      'erpx_sales_write.revenue_arrangement_guards','erpx_finance_write.chart_guards',
      'erpx_finance_write.period_guards','erpx_finance_write.journal_guards',
      'erpx_finance_write.subledger_guards','erpx_finance_write.customer_invoice_guards',
      'erpx_finance_write.revenue_schedule_guards'
    ] LOOP
      EXECUTE format(
        'UPDATE %s SET holding_id=split_part(scope_key,'':'',2)::uuid,
                       company_id=split_part(scope_key,'':'',3)::uuid,
                       branch_id=NULLIF(split_part(scope_key,'':'',4),'''')::uuid
         WHERE holding_id IS NULL OR company_id IS NULL', relation_name);
    END LOOP;
    FOREACH relation_name IN ARRAY ARRAY[
      'erpx_legal_write.contract_guards','erpx_legal_write.document_guards',
      'erpx_legal_write.access_entry_guards','erpx_legal_write.matter_guards'
    ] LOOP
      EXECUTE format(
        'UPDATE %s SET holding_id=split_part(split_part(scope_key,''h:'',2),''/c:'',1)::uuid,
                       company_id=split_part(split_part(scope_key,''/c:'',2),''/b:'',1)::uuid,
                       branch_id=NULLIF(split_part(scope_key,''/b:'',2),'''')::uuid
         WHERE holding_id IS NULL OR company_id IS NULL', relation_name);
    END LOOP;
END $$;
UPDATE erpx_pos_write.sale_guards SET
    holding_id=split_part(split_part(scope_key,'holding:',2),'/company:',1)::uuid,
    company_id=split_part(split_part(scope_key,'/company:',2),'/branch:',1)::uuid,
    branch_id=NULLIF(split_part(scope_key,'/branch:',2),'')::uuid;
UPDATE erpx_pos_write.refund_guards SET
    holding_id=split_part(split_part(scope_key,'holding:',2),'/company:',1)::uuid,
    company_id=split_part(split_part(scope_key,'/company:',2),'/branch:',1)::uuid,
    branch_id=NULLIF(split_part(scope_key,'/branch:',2),'')::uuid;
UPDATE erpx_legal_write.matter_link_guards SET
    target_holding_id=split_part(split_part(target_scope_key,'h:',2),'/c:',1)::uuid,
    target_company_id=split_part(split_part(target_scope_key,'/c:',2),'/b:',1)::uuid,
    target_branch_id=NULLIF(split_part(target_scope_key,'/b:',2),'')::uuid;

CREATE INDEX sales_quote_typed_authority ON erpx_sales_write.quote_guards(holding_id,company_id,branch_id,quote_id);
CREATE INDEX sales_order_typed_authority ON erpx_sales_write.sales_order_guards(holding_id,company_id,branch_id,sales_order_id);
CREATE INDEX sales_arrangement_typed_authority ON erpx_sales_write.revenue_arrangement_guards(holding_id,company_id,branch_id,arrangement_id);
CREATE INDEX pos_sale_typed_authority ON erpx_pos_write.sale_guards(holding_id,company_id,branch_id,sale_id);
CREATE INDEX pos_refund_typed_authority ON erpx_pos_write.refund_guards(holding_id,company_id,branch_id,refund_id);
CREATE INDEX legal_contract_typed_authority ON erpx_legal_write.contract_guards(holding_id,company_id,branch_id,contract_id);
CREATE INDEX legal_document_typed_authority ON erpx_legal_write.document_guards(holding_id,company_id,branch_id,document_id);
CREATE INDEX legal_access_typed_authority ON erpx_legal_write.access_entry_guards(holding_id,company_id,branch_id,access_entry_id);
CREATE INDEX legal_matter_typed_authority ON erpx_legal_write.matter_guards(holding_id,company_id,branch_id,matter_id);
-- END FINANCE_SALES_POS_LEGAL_NORMALIZED_AUTHORITY

-- Identity & Access principal/role authority. Organizational scopes are
-- decomposed into typed scalar/array columns; assignments and memberships are
-- owned relational children.
CREATE TABLE erpx_identity_access_write.principal_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    principal_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    scope_mode TEXT NOT NULL CHECK (scope_mode IN ('platform','holding','company','branch','subtree')),
    holding_id UUID,
    company_id UUID,
    branch_id UUID,
    company_ids UUID[] NOT NULL DEFAULT '{}',
    branch_ids UUID[] NOT NULL DEFAULT '{}',
    display_name TEXT NOT NULL CHECK (octet_length(display_name) BETWEEN 1 AND 256),
    principal_kind TEXT NOT NULL CHECK (
        principal_kind IN ('human','service','agent','human_group','agent_pool')
    ),
    principal_realm TEXT NOT NULL CHECK (
        principal_realm IN ('internal_workforce','platform_workforce','customer_portal')
    ),
    governance_owner_ref TEXT,
    governance_purpose TEXT,
    governance_workload_binding TEXT,
    governance_credential_reference TEXT,
    principal_status TEXT NOT NULL CHECK (
        principal_status IN ('invited','active','suspended','disabled')
    ),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,principal_id),
    UNIQUE (tenant_id,principal_id,aggregate_version),
    CHECK (updated_at >= created_at),
    CHECK (
        (scope_mode='platform' AND holding_id IS NULL AND company_id IS NULL AND branch_id IS NULL)
        OR (scope_mode='holding' AND holding_id IS NOT NULL AND company_id IS NULL AND branch_id IS NULL)
        OR (scope_mode='company' AND holding_id IS NOT NULL AND company_id IS NOT NULL AND branch_id IS NULL)
        OR (scope_mode='branch' AND holding_id IS NOT NULL AND company_id IS NOT NULL AND branch_id IS NOT NULL)
        OR (scope_mode='subtree' AND holding_id IS NOT NULL AND company_id IS NULL AND branch_id IS NULL)
    ),
    CHECK (
        (principal_kind='human' AND governance_owner_ref IS NULL AND governance_purpose IS NULL
            AND governance_workload_binding IS NULL AND governance_credential_reference IS NULL)
        OR (principal_kind<>'human' AND governance_owner_ref IS NOT NULL
            AND governance_purpose IS NOT NULL AND governance_workload_binding IS NOT NULL
            AND governance_credential_reference IS NOT NULL)
    )
);
CREATE INDEX identity_principal_authority_scope
    ON erpx_identity_access_write.principal_authority
       (tenant_id,holding_id,company_id,branch_id,principal_status,updated_at DESC);

CREATE TABLE erpx_identity_access_write.role_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    role_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    scope_mode TEXT NOT NULL CHECK (scope_mode IN ('platform','holding','company','branch','subtree')),
    holding_id UUID,
    company_id UUID,
    branch_id UUID,
    company_ids UUID[] NOT NULL DEFAULT '{}',
    branch_ids UUID[] NOT NULL DEFAULT '{}',
    role_code TEXT NOT NULL CHECK (octet_length(role_code) BETWEEN 1 AND 256),
    display_name TEXT NOT NULL CHECK (octet_length(display_name) BETWEEN 1 AND 256),
    risk_tier TEXT NOT NULL CHECK (risk_tier IN (
        'platform_root','administrator','high','standard','contractor',
        'support_diagnostic','support_mutation','emergency'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,role_id),
    UNIQUE (tenant_id,role_id,aggregate_version),
    UNIQUE (tenant_id,scope_key,role_code),
    CHECK (updated_at >= created_at),
    CHECK (
        (scope_mode='platform' AND holding_id IS NULL AND company_id IS NULL AND branch_id IS NULL)
        OR (scope_mode='holding' AND holding_id IS NOT NULL AND company_id IS NULL AND branch_id IS NULL)
        OR (scope_mode='company' AND holding_id IS NOT NULL AND company_id IS NOT NULL AND branch_id IS NULL)
        OR (scope_mode='branch' AND holding_id IS NOT NULL AND company_id IS NOT NULL AND branch_id IS NOT NULL)
        OR (scope_mode='subtree' AND holding_id IS NOT NULL AND company_id IS NULL AND branch_id IS NULL)
    )
);

CREATE TABLE erpx_identity_access_write.role_permission_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    role_id UUID NOT NULL,
    permission_key TEXT NOT NULL CHECK (octet_length(permission_key) BETWEEN 1 AND 256),
    role_version BIGINT NOT NULL CHECK (role_version > 0),
    PRIMARY KEY (tenant_id,role_id,permission_key),
    FOREIGN KEY (tenant_id,role_id)
        REFERENCES erpx_identity_access_write.role_authority(tenant_id,role_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_identity_access_write.assignment_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    assignment_id UUID NOT NULL,
    principal_id UUID NOT NULL,
    role_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    scope_mode TEXT NOT NULL CHECK (scope_mode IN ('platform','holding','company','branch','subtree')),
    holding_id UUID,
    company_id UUID,
    branch_id UUID,
    company_ids UUID[] NOT NULL DEFAULT '{}',
    branch_ids UUID[] NOT NULL DEFAULT '{}',
    risk_tier TEXT NOT NULL CHECK (risk_tier IN (
        'platform_root','administrator','high','standard','contractor',
        'support_diagnostic','support_mutation','emergency'
    )),
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    grant_reason TEXT NOT NULL CHECK (octet_length(grant_reason) BETWEEN 1 AND 256),
    ticket_reference TEXT CHECK (
        ticket_reference IS NULL OR octet_length(ticket_reference) BETWEEN 1 AND 256
    ),
    grantor_ref TEXT NOT NULL CHECK (octet_length(grantor_ref) BETWEEN 1 AND 256),
    approver_ref TEXT CHECK (approver_ref IS NULL OR octet_length(approver_ref) BETWEEN 1 AND 256),
    revoked_at TIMESTAMPTZ,
    principal_version BIGINT NOT NULL CHECK (principal_version > 0),
    PRIMARY KEY (tenant_id,assignment_id),
    UNIQUE (tenant_id,principal_id,assignment_id),
    FOREIGN KEY (tenant_id,principal_id)
        REFERENCES erpx_identity_access_write.principal_authority(tenant_id,principal_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id,role_id)
        REFERENCES erpx_identity_access_write.role_authority(tenant_id,role_id)
        ON DELETE RESTRICT,
    CHECK (valid_until > valid_from),
    CHECK (revoked_at IS NULL OR revoked_at >= valid_from)
);
CREATE UNIQUE INDEX identity_active_assignment_authority
    ON erpx_identity_access_write.assignment_authority
       (tenant_id,principal_id,role_id,scope_key)
    WHERE revoked_at IS NULL;

CREATE TABLE erpx_identity_access_write.group_membership_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    group_id UUID NOT NULL,
    member_principal_id UUID NOT NULL,
    membership_version BIGINT NOT NULL CHECK (membership_version BETWEEN 1 AND 2),
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    principal_version BIGINT NOT NULL CHECK (principal_version > 0),
    PRIMARY KEY (tenant_id,group_id,member_principal_id),
    FOREIGN KEY (tenant_id,group_id)
        REFERENCES erpx_identity_access_write.principal_authority(tenant_id,principal_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id,member_principal_id)
        REFERENCES erpx_identity_access_write.principal_authority(tenant_id,principal_id)
        ON DELETE RESTRICT,
    CHECK (group_id <> member_principal_id),
    CHECK (valid_until > valid_from),
    CHECK ((membership_version=1) = (revoked_at IS NULL)),
    CHECK (revoked_at IS NULL OR revoked_at >= valid_from)
);

ALTER TABLE erpx_identity_access_write.principal_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_identity_access_write.principal_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY identity_principal_authority_tenant_isolation
    ON erpx_identity_access_write.principal_authority
    USING (tenant_id=erpx_cqrs_write.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id());
ALTER TABLE erpx_identity_access_write.role_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_identity_access_write.role_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY identity_role_authority_tenant_isolation
    ON erpx_identity_access_write.role_authority
    USING (tenant_id=erpx_cqrs_write.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id());
ALTER TABLE erpx_identity_access_write.role_permission_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_identity_access_write.role_permission_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY identity_role_permission_authority_tenant_isolation
    ON erpx_identity_access_write.role_permission_authority
    USING (tenant_id=erpx_cqrs_write.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id());
ALTER TABLE erpx_identity_access_write.assignment_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_identity_access_write.assignment_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY identity_assignment_authority_tenant_isolation
    ON erpx_identity_access_write.assignment_authority
    USING (tenant_id=erpx_cqrs_write.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id());
ALTER TABLE erpx_identity_access_write.group_membership_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_identity_access_write.group_membership_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY identity_group_membership_authority_tenant_isolation
    ON erpx_identity_access_write.group_membership_authority
    USING (tenant_id=erpx_cqrs_write.current_tenant_id())
    WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id());

-- BEGIN PEOPLE_PAYROLL_NORMALIZED_AUTHORITY
-- People & Payroll owner state. Exact organizational scope is decomposed into
-- scalar columns; payroll lines and calculation evidence are relational
-- children rather than a serialized aggregate document.
CREATE TABLE erpx_people_payroll_write.employee_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    employee_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    employee_code TEXT NOT NULL CHECK (octet_length(employee_code) BETWEEN 1 AND 128),
    display_name TEXT NOT NULL CHECK (octet_length(display_name) BETWEEN 1 AND 255),
    hired_on DATE NOT NULL,
    terminated_on DATE,
    employee_status TEXT NOT NULL CHECK (
        employee_status IN ('draft','active','on_leave','suspended','terminated')
    ),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,employee_id),
    UNIQUE (tenant_id,scope_key,employee_code),
    CHECK (updated_at >= created_at),
    CHECK (
        (employee_status='terminated' AND terminated_on IS NOT NULL AND terminated_on >= hired_on)
        OR (employee_status<>'terminated' AND terminated_on IS NULL)
    )
);

CREATE TABLE erpx_people_payroll_write.leave_request_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    leave_request_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    employee_id UUID NOT NULL,
    starts_on DATE NOT NULL,
    ends_on DATE NOT NULL,
    reason TEXT NOT NULL CHECK (octet_length(reason) BETWEEN 1 AND 1024),
    leave_status TEXT NOT NULL CHECK (
        leave_status IN ('draft','submitted','approved','rejected','cancelled','taken')
    ),
    requested_by TEXT NOT NULL CHECK (octet_length(requested_by) BETWEEN 1 AND 128),
    approved_by TEXT CHECK (approved_by IS NULL OR octet_length(approved_by) BETWEEN 1 AND 128),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,leave_request_id),
    FOREIGN KEY (tenant_id,employee_id)
        REFERENCES erpx_people_payroll_write.employee_authority(tenant_id,employee_id)
        ON DELETE RESTRICT,
    CHECK (ends_on >= starts_on),
    CHECK (updated_at >= created_at),
    CHECK (approved_by IS NULL OR approved_by <> requested_by),
    CHECK (leave_status NOT IN ('approved','taken') OR approved_by IS NOT NULL)
);

CREATE TABLE erpx_people_payroll_write.expense_claim_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    expense_claim_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    employee_id UUID NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    amount_minor BIGINT NOT NULL CHECK (amount_minor >= 0),
    reason TEXT NOT NULL CHECK (octet_length(reason) BETWEEN 1 AND 1024),
    expense_status TEXT NOT NULL CHECK (
        expense_status IN ('draft','submitted','approved','rejected','reimbursed')
    ),
    claimed_by TEXT NOT NULL CHECK (octet_length(claimed_by) BETWEEN 1 AND 128),
    approved_by TEXT CHECK (approved_by IS NULL OR octet_length(approved_by) BETWEEN 1 AND 128),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,expense_claim_id),
    FOREIGN KEY (tenant_id,employee_id)
        REFERENCES erpx_people_payroll_write.employee_authority(tenant_id,employee_id)
        ON DELETE RESTRICT,
    CHECK (updated_at >= created_at),
    CHECK (approved_by IS NULL OR approved_by <> claimed_by),
    CHECK (expense_status NOT IN ('approved','reimbursed') OR approved_by IS NOT NULL)
);

CREATE TABLE erpx_people_payroll_write.payroll_run_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    payroll_run_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    period_code TEXT NOT NULL CHECK (octet_length(period_code) BETWEEN 1 AND 128),
    starts_on DATE NOT NULL,
    ends_on DATE NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    payroll_status TEXT NOT NULL CHECK (
        payroll_status IN ('draft','review','approved','posted','cancelled')
    ),
    prepared_by TEXT NOT NULL CHECK (octet_length(prepared_by) BETWEEN 1 AND 128),
    reviewed_by TEXT CHECK (reviewed_by IS NULL OR octet_length(reviewed_by) BETWEEN 1 AND 128),
    approved_by TEXT CHECK (approved_by IS NULL OR octet_length(approved_by) BETWEEN 1 AND 128),
    total_gross_minor BIGINT NOT NULL CHECK (total_gross_minor >= 0),
    total_deductions_minor BIGINT NOT NULL CHECK (total_deductions_minor >= 0),
    total_net_minor BIGINT NOT NULL CHECK (total_net_minor >= 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,payroll_run_id),
    UNIQUE (tenant_id,scope_key,period_code),
    CHECK (ends_on >= starts_on),
    CHECK (updated_at >= created_at),
    CHECK (total_gross_minor-total_deductions_minor=total_net_minor),
    CHECK (reviewed_by IS NULL OR reviewed_by<>prepared_by),
    CHECK (approved_by IS NULL OR (approved_by<>prepared_by AND approved_by IS DISTINCT FROM reviewed_by)),
    CHECK (payroll_status NOT IN ('approved','posted') OR reviewed_by IS NOT NULL),
    CHECK (payroll_status<>'posted' OR approved_by IS NOT NULL)
);

CREATE TABLE erpx_people_payroll_write.payroll_line_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    payroll_run_id UUID NOT NULL,
    employee_id UUID NOT NULL,
    gross_minor BIGINT NOT NULL CHECK (gross_minor >= 0),
    deductions_minor BIGINT NOT NULL CHECK (deductions_minor >= 0),
    net_minor BIGINT NOT NULL CHECK (net_minor >= 0),
    payroll_version BIGINT NOT NULL CHECK (payroll_version > 0),
    PRIMARY KEY (tenant_id,payroll_run_id,employee_id),
    FOREIGN KEY (tenant_id,payroll_run_id)
        REFERENCES erpx_people_payroll_write.payroll_run_authority(tenant_id,payroll_run_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id,employee_id)
        REFERENCES erpx_people_payroll_write.employee_authority(tenant_id,employee_id)
        ON DELETE RESTRICT,
    CHECK (gross_minor-deductions_minor=net_minor)
);

CREATE TABLE erpx_people_payroll_write.payroll_calculation_evidence_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    payroll_run_id UUID NOT NULL,
    policy_version TEXT NOT NULL CHECK (octet_length(policy_version) BETWEEN 1 AND 128),
    rate_table_version TEXT NOT NULL CHECK (octet_length(rate_table_version) BETWEEN 1 AND 128),
    calculation_sha256 TEXT NOT NULL CHECK (calculation_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    captured_at TIMESTAMPTZ NOT NULL,
    payroll_version BIGINT NOT NULL CHECK (payroll_version > 0),
    PRIMARY KEY (tenant_id,payroll_run_id),
    FOREIGN KEY (tenant_id,payroll_run_id)
        REFERENCES erpx_people_payroll_write.payroll_run_authority(tenant_id,payroll_run_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_people_payroll_write.payroll_input_evidence_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    payroll_run_id UUID NOT NULL,
    input_ordinal INTEGER NOT NULL CHECK (input_ordinal >= 0),
    source_record_id UUID NOT NULL,
    source_owner_context TEXT NOT NULL CHECK (octet_length(source_owner_context) BETWEEN 1 AND 64),
    source_record_type TEXT NOT NULL CHECK (octet_length(source_record_type) BETWEEN 1 AND 128),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    content_sha256 TEXT NOT NULL CHECK (content_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    payroll_version BIGINT NOT NULL CHECK (payroll_version > 0),
    PRIMARY KEY (tenant_id,payroll_run_id,input_ordinal),
    UNIQUE (tenant_id,payroll_run_id,source_record_id,source_version),
    FOREIGN KEY (tenant_id,payroll_run_id)
        REFERENCES erpx_people_payroll_write.payroll_run_authority(tenant_id,payroll_run_id)
        ON DELETE RESTRICT
);

CREATE INDEX people_employee_authority_scope
    ON erpx_people_payroll_write.employee_authority
       (tenant_id,holding_id,company_id,branch_id,employee_status,updated_at DESC);
CREATE INDEX people_leave_authority_scope
    ON erpx_people_payroll_write.leave_request_authority
       (tenant_id,holding_id,company_id,branch_id,leave_status,updated_at DESC);
CREATE INDEX people_expense_authority_scope
    ON erpx_people_payroll_write.expense_claim_authority
       (tenant_id,holding_id,company_id,branch_id,expense_status,updated_at DESC);
CREATE INDEX payroll_run_authority_scope
    ON erpx_people_payroll_write.payroll_run_authority
       (tenant_id,holding_id,company_id,branch_id,payroll_status,updated_at DESC);

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'employee_authority','leave_request_authority','expense_claim_authority',
        'payroll_run_authority','payroll_line_authority',
        'payroll_calculation_evidence_authority','payroll_input_evidence_authority'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_people_payroll_write.%I ENABLE ROW LEVEL SECURITY',table_name);
        EXECUTE format('ALTER TABLE erpx_people_payroll_write.%I FORCE ROW LEVEL SECURITY',table_name);
        EXECUTE format(
            'CREATE POLICY people_payroll_%I_tenant_isolation ON erpx_people_payroll_write.%I USING (tenant_id=erpx_cqrs_write.current_tenant_id()) WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id())',
            table_name,table_name
        );
    END LOOP;
END $$;
-- END PEOPLE_PAYROLL_NORMALIZED_AUTHORITY

-- BEGIN OPERATIONS_NORMALIZED_AUTHORITY
-- Operations exception and policy authority. The inbound SourceFact union is
-- normalized as one discriminator plus exactly one source-specific relation;
-- list-valued facts use owned child rows.
CREATE TABLE erpx_operations_write.exception_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    exception_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    scope_mode TEXT NOT NULL CHECK (scope_mode IN ('holding','company','branch')),
    holding_id UUID NOT NULL,
    company_id UUID,
    branch_id UUID,
    reference TEXT NOT NULL CHECK (octet_length(reference) BETWEEN 1 AND 256),
    source_context TEXT NOT NULL,
    source_event_id UUID NOT NULL,
    source_event_type TEXT NOT NULL CHECK (octet_length(source_event_type) BETWEEN 1 AND 256),
    source_aggregate_type TEXT NOT NULL CHECK (octet_length(source_aggregate_type) BETWEEN 1 AND 256),
    source_aggregate_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_correlation_id UUID NOT NULL,
    source_occurred_at TIMESTAMPTZ NOT NULL,
    policy_ref TEXT NOT NULL CHECK (octet_length(policy_ref) BETWEEN 1 AND 128),
    category TEXT NOT NULL CHECK (octet_length(category) BETWEEN 1 AND 128),
    severity TEXT NOT NULL CHECK (octet_length(severity) BETWEEN 1 AND 128),
    assignee_id UUID NOT NULL,
    manager_id UUID NOT NULL,
    assignment_authority_ref TEXT NOT NULL CHECK (octet_length(assignment_authority_ref) BETWEEN 1 AND 256),
    assignment_valid_from TIMESTAMPTZ NOT NULL,
    assignment_valid_until TIMESTAMPTZ NOT NULL,
    due_at TIMESTAMPTZ NOT NULL,
    escalation_after_minutes INTEGER NOT NULL CHECK (escalation_after_minutes BETWEEN 1 AND 10080),
    exception_status TEXT NOT NULL CHECK (exception_status IN ('assigned','acknowledged','escalated','resolved','closed')),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,exception_id),
    UNIQUE (tenant_id,scope_key,reference),
    UNIQUE (tenant_id,source_context,source_event_id),
    CHECK (updated_at >= created_at),
    CHECK (assignment_valid_until > assignment_valid_from),
    CHECK (assignee_id <> manager_id),
    CHECK (
        (scope_mode='holding' AND company_id IS NULL AND branch_id IS NULL)
        OR (scope_mode='company' AND company_id IS NOT NULL AND branch_id IS NULL)
        OR (scope_mode='branch' AND company_id IS NOT NULL AND branch_id IS NOT NULL)
    )
);

CREATE TABLE erpx_operations_write.exception_source_fact_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    exception_id UUID NOT NULL,
    fact_kind TEXT NOT NULL CHECK (fact_kind IN (
        'commercial','customer','marketing','marketing_budget_approval','crm','procurement',
        'inventory','assets','maintenance','quality','sales','people_payroll','pos_sale',
        'pos_refund','finance','legal','support','support_incident'
    )),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    PRIMARY KEY (tenant_id,exception_id),
    FOREIGN KEY (tenant_id,exception_id)
        REFERENCES erpx_operations_write.exception_authority(tenant_id,exception_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_operations_write.source_commercial_authority (
    tenant_id UUID NOT NULL, exception_id UUID NOT NULL,
    subscription_id UUID NOT NULL, customer_id UUID NOT NULL, plan_id UUID NOT NULL,
    subscription_status TEXT NOT NULL, recurring_currency TEXT NOT NULL CHECK (recurring_currency ~ '^[A-Z]{3}$'),
    recurring_minor BIGINT NOT NULL, effective_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,exception_id),
    FOREIGN KEY (tenant_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(tenant_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_customer_authority (
    tenant_id UUID NOT NULL, exception_id UUID NOT NULL,
    customer_id UUID NOT NULL, account_id UUID NOT NULL, customer_code TEXT NOT NULL,
    display_name TEXT NOT NULL, owning_company_id UUID NOT NULL, customer_status TEXT NOT NULL,
    PRIMARY KEY (tenant_id,exception_id),
    FOREIGN KEY (tenant_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(tenant_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_marketing_authority (
    tenant_id UUID NOT NULL, exception_id UUID NOT NULL,
    campaign_id UUID NOT NULL, audience_member_id UUID NOT NULL,
    channel_code TEXT NOT NULL, action_code TEXT NOT NULL,
    PRIMARY KEY (tenant_id,exception_id),
    FOREIGN KEY (tenant_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(tenant_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_marketing_approval_authority (
    tenant_id UUID NOT NULL, exception_id UUID NOT NULL,
    budget_id UUID NOT NULL, campaign_id UUID NOT NULL, approval_request_id UUID NOT NULL,
    subject_record_id UUID NOT NULL, subject_context TEXT NOT NULL,
    subject_aggregate_kind TEXT NOT NULL, subject_aggregate_version BIGINT NOT NULL CHECK (subject_aggregate_version > 0),
    policy_code TEXT NOT NULL, amount_currency TEXT CHECK (amount_currency IS NULL OR amount_currency ~ '^[A-Z]{3}$'),
    amount_minor BIGINT, requested_by TEXT NOT NULL, requested_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,exception_id),
    FOREIGN KEY (tenant_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(tenant_id,exception_id) ON DELETE RESTRICT,
    CHECK ((amount_currency IS NULL)=(amount_minor IS NULL))
);
CREATE TABLE erpx_operations_write.source_crm_authority (
    tenant_id UUID NOT NULL, exception_id UUID NOT NULL,
    opportunity_id UUID NOT NULL, customer_id UUID NOT NULL, opportunity_stage TEXT NOT NULL,
    expected_currency TEXT NOT NULL CHECK (expected_currency ~ '^[A-Z]{3}$'), expected_minor BIGINT NOT NULL,
    PRIMARY KEY (tenant_id,exception_id),
    FOREIGN KEY (tenant_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(tenant_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_procurement_authority (
    tenant_id UUID NOT NULL, exception_id UUID NOT NULL,
    purchase_order_id UUID NOT NULL, supplier_id UUID NOT NULL, order_code TEXT NOT NULL,
    PRIMARY KEY (tenant_id,exception_id),
    FOREIGN KEY (tenant_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(tenant_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_procurement_line_authority (
    tenant_id UUID NOT NULL, exception_id UUID NOT NULL, line_ordinal INTEGER NOT NULL CHECK (line_ordinal>=0),
    line_id UUID NOT NULL, item_code TEXT NOT NULL, quantity_units BIGINT NOT NULL,
    quantity_scale SMALLINT NOT NULL CHECK (quantity_scale BETWEEN 0 AND 9),
    line_currency TEXT NOT NULL CHECK (line_currency ~ '^[A-Z]{3}$'), line_minor BIGINT NOT NULL,
    destination_code TEXT NOT NULL,
    PRIMARY KEY (tenant_id,exception_id,line_ordinal),
    UNIQUE (tenant_id,exception_id,line_id),
    FOREIGN KEY (tenant_id,exception_id) REFERENCES erpx_operations_write.source_procurement_authority(tenant_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_inventory_authority (
    tenant_id UUID NOT NULL, exception_id UUID NOT NULL,
    movement_id UUID NOT NULL, stock_item_id UUID NOT NULL, warehouse_id UUID NOT NULL,
    item_code TEXT NOT NULL, quantity_units BIGINT NOT NULL,
    quantity_scale SMALLINT NOT NULL CHECK (quantity_scale BETWEEN 0 AND 9), procurement_line_id UUID,
    PRIMARY KEY (tenant_id,exception_id),
    FOREIGN KEY (tenant_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(tenant_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_assets_authority (
    tenant_id UUID NOT NULL, exception_id UUID NOT NULL,
    asset_id UUID NOT NULL, asset_code TEXT NOT NULL, asset_name TEXT NOT NULL,
    location_id UUID NOT NULL, asset_condition TEXT NOT NULL,
    PRIMARY KEY (tenant_id,exception_id),
    FOREIGN KEY (tenant_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(tenant_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_maintenance_authority (
    tenant_id UUID NOT NULL, exception_id UUID NOT NULL,
    work_order_id UUID NOT NULL, work_order_code TEXT NOT NULL, asset_id UUID NOT NULL,
    work_order_status TEXT NOT NULL, scheduled_for TIMESTAMPTZ,
    PRIMARY KEY (tenant_id,exception_id),
    FOREIGN KEY (tenant_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(tenant_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_quality_authority (
    tenant_id UUID NOT NULL, exception_id UUID NOT NULL,
    inspection_id UUID NOT NULL, subject_id UUID NOT NULL, inspection_code TEXT NOT NULL,
    inspection_outcome TEXT NOT NULL,
    PRIMARY KEY (tenant_id,exception_id),
    FOREIGN KEY (tenant_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(tenant_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_sales_authority (
    tenant_id UUID NOT NULL, exception_id UUID NOT NULL,
    sales_order_id UUID NOT NULL, order_code TEXT NOT NULL, customer_id UUID NOT NULL,
    sales_status TEXT NOT NULL, total_currency TEXT NOT NULL CHECK (total_currency ~ '^[A-Z]{3}$'),
    total_minor BIGINT NOT NULL,
    PRIMARY KEY (tenant_id,exception_id),
    FOREIGN KEY (tenant_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(tenant_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_sales_line_authority (
    tenant_id UUID NOT NULL, exception_id UUID NOT NULL, line_ordinal INTEGER NOT NULL CHECK (line_ordinal>=0),
    line_id UUID NOT NULL, item_code TEXT NOT NULL, quantity_units BIGINT NOT NULL,
    quantity_scale SMALLINT NOT NULL CHECK (quantity_scale BETWEEN 0 AND 9),
    line_currency TEXT NOT NULL CHECK (line_currency ~ '^[A-Z]{3}$'), line_minor BIGINT NOT NULL,
    PRIMARY KEY (tenant_id,exception_id,line_ordinal),
    UNIQUE (tenant_id,exception_id,line_id),
    FOREIGN KEY (tenant_id,exception_id) REFERENCES erpx_operations_write.source_sales_authority(tenant_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_people_payroll_authority (
    tenant_id UUID NOT NULL, exception_id UUID NOT NULL,
    payroll_run_id UUID NOT NULL, period_code TEXT NOT NULL, finalized_at TIMESTAMPTZ NOT NULL,
    gross_currency TEXT NOT NULL CHECK (gross_currency ~ '^[A-Z]{3}$'), gross_minor BIGINT NOT NULL,
    deduction_currency TEXT NOT NULL CHECK (deduction_currency ~ '^[A-Z]{3}$'), deduction_minor BIGINT NOT NULL,
    net_currency TEXT NOT NULL CHECK (net_currency ~ '^[A-Z]{3}$'), net_minor BIGINT NOT NULL,
    PRIMARY KEY (tenant_id,exception_id),
    FOREIGN KEY (tenant_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(tenant_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_pos_sale_authority (
    tenant_id UUID NOT NULL, exception_id UUID NOT NULL,
    transaction_id UUID NOT NULL, register_id UUID NOT NULL, receipt_code TEXT NOT NULL,
    customer_id UUID, gross_currency TEXT NOT NULL CHECK (gross_currency ~ '^[A-Z]{3}$'),
    gross_minor BIGINT NOT NULL, completed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,exception_id),
    FOREIGN KEY (tenant_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(tenant_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_pos_refund_authority (
    tenant_id UUID NOT NULL, exception_id UUID NOT NULL,
    refund_id UUID NOT NULL, original_sale_id UUID NOT NULL,
    amount_currency TEXT NOT NULL CHECK (amount_currency ~ '^[A-Z]{3}$'), amount_minor BIGINT NOT NULL,
    approved_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,exception_id),
    FOREIGN KEY (tenant_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(tenant_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_finance_authority (
    tenant_id UUID NOT NULL, exception_id UUID NOT NULL,
    journal_id UUID NOT NULL, journal_code TEXT NOT NULL, journal_source_context TEXT NOT NULL,
    source_reference TEXT NOT NULL,
    debit_currency TEXT NOT NULL CHECK (debit_currency ~ '^[A-Z]{3}$'), debit_minor BIGINT NOT NULL,
    credit_currency TEXT NOT NULL CHECK (credit_currency ~ '^[A-Z]{3}$'), credit_minor BIGINT NOT NULL,
    posted_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,exception_id),
    FOREIGN KEY (tenant_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(tenant_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_legal_authority (
    tenant_id UUID NOT NULL, exception_id UUID NOT NULL,
    obligation_id UUID NOT NULL, legal_case_id UUID NOT NULL, customer_id UUID,
    obligation_code TEXT NOT NULL, obligation_title TEXT NOT NULL, obligation_status TEXT NOT NULL,
    due_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id,exception_id),
    FOREIGN KEY (tenant_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(tenant_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_support_authority (
    tenant_id UUID NOT NULL, exception_id UUID NOT NULL,
    support_case_id UUID NOT NULL, case_code TEXT NOT NULL, customer_id UUID NOT NULL,
    support_status TEXT NOT NULL, category_code TEXT NOT NULL, source_updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,exception_id),
    FOREIGN KEY (tenant_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(tenant_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_support_incident_authority (
    tenant_id UUID NOT NULL, exception_id UUID NOT NULL,
    incident_id UUID NOT NULL, incident_code TEXT NOT NULL, incident_status TEXT NOT NULL,
    severity_code TEXT NOT NULL, source_updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,exception_id),
    FOREIGN KEY (tenant_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(tenant_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_support_service_authority (
    tenant_id UUID NOT NULL, exception_id UUID NOT NULL, service_ordinal INTEGER NOT NULL CHECK (service_ordinal>=0),
    service_code TEXT NOT NULL,
    PRIMARY KEY (tenant_id,exception_id,service_ordinal),
    UNIQUE (tenant_id,exception_id,service_code),
    FOREIGN KEY (tenant_id,exception_id) REFERENCES erpx_operations_write.source_support_incident_authority(tenant_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_support_customer_authority (
    tenant_id UUID NOT NULL, exception_id UUID NOT NULL, customer_ordinal INTEGER NOT NULL CHECK (customer_ordinal>=0),
    customer_account_id UUID NOT NULL,
    PRIMARY KEY (tenant_id,exception_id,customer_ordinal),
    UNIQUE (tenant_id,exception_id,customer_account_id),
    FOREIGN KEY (tenant_id,exception_id) REFERENCES erpx_operations_write.source_support_incident_authority(tenant_id,exception_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_operations_write.work_queue_policy_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    policy_id UUID NOT NULL, aggregate_version BIGINT NOT NULL CHECK (aggregate_version>0),
    scope_key TEXT NOT NULL, holding_id UUID NOT NULL, policy_name TEXT NOT NULL,
    escalation_after_minutes INTEGER NOT NULL CHECK (escalation_after_minutes BETWEEN 1 AND 10080),
    policy_status TEXT NOT NULL CHECK (policy_status IN ('active','paused','retired')),
    created_at TIMESTAMPTZ NOT NULL, updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,policy_id), UNIQUE (tenant_id,scope_key,policy_name),
    CHECK (updated_at>=created_at)
);
CREATE TABLE erpx_operations_write.work_queue_policy_source_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    policy_id UUID NOT NULL, source_context TEXT NOT NULL, policy_version BIGINT NOT NULL CHECK (policy_version>0),
    PRIMARY KEY (tenant_id,policy_id,source_context),
    FOREIGN KEY (tenant_id,policy_id) REFERENCES erpx_operations_write.work_queue_policy_authority(tenant_id,policy_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_operations_write.source_receipt_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    source_event_id UUID NOT NULL,
    source_context TEXT NOT NULL,
    source_fingerprint BYTEA NOT NULL CHECK (octet_length(source_fingerprint)=32),
    disposition TEXT NOT NULL CHECK (disposition IN ('ignored','detected')),
    exception_id UUID,
    applied_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,source_event_id),
    FOREIGN KEY (tenant_id,exception_id)
        REFERENCES erpx_operations_write.exception_authority(tenant_id,exception_id)
        ON DELETE RESTRICT,
    CHECK ((disposition='ignored')=(exception_id IS NULL))
);
CREATE TABLE erpx_operations_write.source_checkpoint_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    source_database TEXT NOT NULL CHECK (source_database IN ('control','tenant')),
    source_key TEXT NOT NULL CHECK (octet_length(source_key) BETWEEN 1 AND 255),
    source_partition TEXT NOT NULL,
    source_sequence BIGINT NOT NULL CHECK (source_sequence >= 0),
    source_event_id UUID,
    source_occurred_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,source_database,source_key,source_partition),
    CHECK ((source_sequence=0)=(source_event_id IS NULL))
);

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'exception_authority','exception_source_fact_authority','source_commercial_authority',
        'source_customer_authority','source_marketing_authority','source_marketing_approval_authority',
        'source_crm_authority','source_procurement_authority','source_procurement_line_authority',
        'source_inventory_authority','source_assets_authority','source_maintenance_authority',
        'source_quality_authority','source_sales_authority','source_sales_line_authority',
        'source_people_payroll_authority','source_pos_sale_authority','source_pos_refund_authority',
        'source_finance_authority','source_legal_authority','source_support_authority',
        'source_support_incident_authority','source_support_service_authority',
        'source_support_customer_authority','work_queue_policy_authority',
        'work_queue_policy_source_authority','source_receipt_authority',
        'source_checkpoint_authority'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_operations_write.%I ENABLE ROW LEVEL SECURITY',table_name);
        EXECUTE format('ALTER TABLE erpx_operations_write.%I FORCE ROW LEVEL SECURITY',table_name);
        EXECUTE format(
            'CREATE POLICY operations_%I_tenant_isolation ON erpx_operations_write.%I USING (tenant_id=erpx_cqrs_write.current_tenant_id()) WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id())',
            table_name,table_name
        );
    END LOOP;
END $$;
-- END OPERATIONS_NORMALIZED_AUTHORITY

-- BEGIN SUPPORT_NORMALIZED_AUTHORITY
-- Support customer-case, incident, access-session, and capability command
-- authority. Earlier JSON guard columns remain generation-one evidence only;
-- current command rehydration uses these typed roots and child relations.

CREATE TABLE erpx_support_write.ticket_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    ticket_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    customer_id UUID NOT NULL,
    customer_account_id UUID NOT NULL,
    case_code TEXT NOT NULL CHECK (octet_length(case_code) BETWEEN 1 AND 128),
    category TEXT NOT NULL CHECK (octet_length(category) BETWEEN 1 AND 128),
    severity TEXT NOT NULL CHECK (octet_length(severity) BETWEEN 1 AND 128),
    description TEXT NOT NULL CHECK (octet_length(description) BETWEEN 1 AND 16000),
    ticket_status TEXT NOT NULL CHECK (ticket_status IN (
        'new','triaged','waiting_for_customer','access_requested','approved',
        'in_progress','resolved','customer_confirmed','closed'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,ticket_id),
    UNIQUE (tenant_id,ticket_id,aggregate_version),
    UNIQUE (tenant_id,scope_key,case_code),
    CHECK (updated_at >= created_at)
);

CREATE TABLE erpx_support_write.ticket_message_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    ticket_id UUID NOT NULL,
    message_ordinal INTEGER NOT NULL CHECK (message_ordinal >= 0),
    message_id UUID NOT NULL,
    author_ref TEXT NOT NULL CHECK (octet_length(author_ref) BETWEEN 1 AND 256),
    audience TEXT NOT NULL CHECK (audience IN ('customer_visible','internal')),
    body TEXT NOT NULL CHECK (octet_length(body) BETWEEN 1 AND 16000),
    created_at TIMESTAMPTZ NOT NULL,
    ticket_version BIGINT NOT NULL CHECK (ticket_version > 0),
    PRIMARY KEY (tenant_id,ticket_id,message_ordinal),
    UNIQUE (tenant_id,ticket_id,message_id),
    FOREIGN KEY (tenant_id,ticket_id)
        REFERENCES erpx_support_write.ticket_authority(tenant_id,ticket_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_support_write.incident_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    incident_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    incident_code TEXT NOT NULL CHECK (octet_length(incident_code) BETWEEN 1 AND 128),
    severity TEXT NOT NULL CHECK (octet_length(severity) BETWEEN 1 AND 128),
    commander_ref TEXT NOT NULL CHECK (octet_length(commander_ref) BETWEEN 1 AND 256),
    incident_status TEXT NOT NULL CHECK (incident_status IN (
        'investigating','identified','monitoring','resolved','postmortem_complete'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,incident_id),
    UNIQUE (tenant_id,incident_id,aggregate_version),
    UNIQUE (tenant_id,scope_key,incident_code),
    CHECK (updated_at >= created_at)
);

CREATE TABLE erpx_support_write.incident_service_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    incident_id UUID NOT NULL,
    service_ordinal INTEGER NOT NULL CHECK (service_ordinal >= 0),
    service_ref TEXT NOT NULL CHECK (octet_length(service_ref) BETWEEN 1 AND 256),
    incident_version BIGINT NOT NULL CHECK (incident_version > 0),
    PRIMARY KEY (tenant_id,incident_id,service_ordinal),
    UNIQUE (tenant_id,incident_id,service_ref),
    FOREIGN KEY (tenant_id,incident_id)
        REFERENCES erpx_support_write.incident_authority(tenant_id,incident_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_support_write.incident_customer_account_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    incident_id UUID NOT NULL,
    account_ordinal INTEGER NOT NULL CHECK (account_ordinal >= 0),
    customer_account_id UUID NOT NULL,
    incident_version BIGINT NOT NULL CHECK (incident_version > 0),
    PRIMARY KEY (tenant_id,incident_id,account_ordinal),
    FOREIGN KEY (tenant_id,incident_id)
        REFERENCES erpx_support_write.incident_authority(tenant_id,incident_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_support_write.incident_timeline_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    incident_id UUID NOT NULL,
    timeline_ordinal INTEGER NOT NULL CHECK (timeline_ordinal >= 0),
    occurred_at TIMESTAMPTZ NOT NULL,
    actor_ref TEXT NOT NULL CHECK (octet_length(actor_ref) BETWEEN 1 AND 256),
    message TEXT NOT NULL CHECK (octet_length(message) BETWEEN 1 AND 2000),
    incident_version BIGINT NOT NULL CHECK (incident_version > 0),
    PRIMARY KEY (tenant_id,incident_id,timeline_ordinal),
    FOREIGN KEY (tenant_id,incident_id)
        REFERENCES erpx_support_write.incident_authority(tenant_id,incident_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_support_write.incident_broadcast_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    incident_id UUID NOT NULL,
    broadcast_ordinal INTEGER NOT NULL CHECK (broadcast_ordinal >= 0),
    occurred_at TIMESTAMPTZ NOT NULL,
    actor_ref TEXT NOT NULL CHECK (octet_length(actor_ref) BETWEEN 1 AND 256),
    audience TEXT NOT NULL CHECK (audience IN ('customer_visible','internal')),
    message TEXT NOT NULL CHECK (octet_length(message) BETWEEN 1 AND 2000),
    incident_version BIGINT NOT NULL CHECK (incident_version > 0),
    PRIMARY KEY (tenant_id,incident_id,broadcast_ordinal),
    FOREIGN KEY (tenant_id,incident_id)
        REFERENCES erpx_support_write.incident_authority(tenant_id,incident_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_support_write.session_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    session_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    ticket_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    customer_account_id UUID NOT NULL,
    agent_principal_id UUID NOT NULL,
    represented_principal_id UUID,
    support_mode TEXT NOT NULL CHECK (support_mode IN (
        'diagnostic_read','tenant_mutation','impersonation'
    )),
    module TEXT NOT NULL CHECK (octet_length(module) BETWEEN 1 AND 128),
    reason TEXT NOT NULL CHECK (octet_length(reason) BETWEEN 1 AND 2000),
    session_status TEXT NOT NULL CHECK (session_status IN ('requested','approved','revoked')),
    requested_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    approved_by TEXT CHECK (approved_by IS NULL OR octet_length(approved_by) BETWEEN 1 AND 256),
    approved_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    revoked_reason TEXT CHECK (
        revoked_reason IS NULL OR octet_length(revoked_reason) BETWEEN 1 AND 2000
    ),
    PRIMARY KEY (tenant_id,session_id),
    UNIQUE (tenant_id,session_id,aggregate_version),
    FOREIGN KEY (tenant_id,ticket_id)
        REFERENCES erpx_support_write.ticket_authority(tenant_id,ticket_id)
        ON DELETE RESTRICT,
    CHECK (expires_at > requested_at),
    CHECK ((support_mode='impersonation') = (represented_principal_id IS NOT NULL)),
    CHECK ((approved_by IS NULL) = (approved_at IS NULL)),
    CHECK (approved_by IS NULL OR approved_by <> agent_principal_id::TEXT),
    CHECK ((revoked_at IS NULL) = (revoked_reason IS NULL)),
    CHECK (
        (session_status='requested' AND aggregate_version=1
            AND approved_at IS NULL AND revoked_at IS NULL)
        OR (session_status='approved' AND aggregate_version=2
            AND approved_at IS NOT NULL AND revoked_at IS NULL)
        OR (session_status='revoked' AND aggregate_version IN (2,3)
            AND revoked_at IS NOT NULL)
    )
);

CREATE TABLE erpx_support_write.session_permission_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    session_id UUID NOT NULL,
    permission_ordinal INTEGER NOT NULL CHECK (permission_ordinal >= 0),
    permission TEXT NOT NULL CHECK (octet_length(permission) BETWEEN 1 AND 256),
    session_version BIGINT NOT NULL CHECK (session_version > 0),
    PRIMARY KEY (tenant_id,session_id,permission_ordinal),
    UNIQUE (tenant_id,session_id,permission),
    FOREIGN KEY (tenant_id,session_id)
        REFERENCES erpx_support_write.session_authority(tenant_id,session_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_support_write.capability_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    capability_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version IN (1,2)),
    scope_key TEXT NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    session_id UUID NOT NULL,
    ticket_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    customer_account_id UUID NOT NULL,
    agent_principal_id UUID NOT NULL,
    device_ref TEXT NOT NULL CHECK (octet_length(device_ref) BETWEEN 1 AND 256),
    support_mode TEXT NOT NULL CHECK (support_mode IN (
        'diagnostic_read','tenant_mutation','impersonation'
    )),
    module TEXT NOT NULL CHECK (octet_length(module) BETWEEN 1 AND 128),
    secret_hash_sha256 BYTEA NOT NULL CHECK (octet_length(secret_hash_sha256)=32),
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revocation_reason TEXT CHECK (revocation_reason IN (
        'customer_requested','agent_disabled','device_revoked','session_revoked',
        'ticket_closed','administrative'
    )),
    revoked_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id,capability_id),
    UNIQUE (tenant_id,capability_id,aggregate_version),
    FOREIGN KEY (tenant_id,session_id)
        REFERENCES erpx_support_write.session_authority(tenant_id,session_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id,ticket_id)
        REFERENCES erpx_support_write.ticket_authority(tenant_id,ticket_id)
        ON DELETE RESTRICT,
    CHECK (expires_at > issued_at),
    CHECK ((revocation_reason IS NULL) = (revoked_at IS NULL)),
    CHECK (
        (aggregate_version=1 AND revoked_at IS NULL)
        OR (aggregate_version=2 AND revoked_at IS NOT NULL)
    )
);

CREATE TABLE erpx_support_write.capability_permission_authority (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_tenant_id(),
    capability_id UUID NOT NULL,
    permission_ordinal INTEGER NOT NULL CHECK (permission_ordinal >= 0),
    permission TEXT NOT NULL CHECK (octet_length(permission) BETWEEN 1 AND 256),
    capability_version BIGINT NOT NULL CHECK (capability_version IN (1,2)),
    PRIMARY KEY (tenant_id,capability_id,permission_ordinal),
    UNIQUE (tenant_id,capability_id,permission),
    FOREIGN KEY (tenant_id,capability_id)
        REFERENCES erpx_support_write.capability_authority(tenant_id,capability_id)
        ON DELETE RESTRICT
);

CREATE INDEX support_ticket_authority_scope
    ON erpx_support_write.ticket_authority
        (tenant_id,holding_id,company_id,branch_id,customer_id,ticket_status,updated_at DESC);
CREATE INDEX support_incident_authority_scope
    ON erpx_support_write.incident_authority
        (tenant_id,holding_id,company_id,branch_id,incident_status,updated_at DESC);
CREATE INDEX support_session_authority_scope
    ON erpx_support_write.session_authority
        (tenant_id,holding_id,company_id,branch_id,ticket_id,session_status,expires_at DESC);
CREATE INDEX support_capability_authority_scope
    ON erpx_support_write.capability_authority
        (tenant_id,holding_id,company_id,branch_id,session_id,expires_at DESC);

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'ticket_authority','ticket_message_authority','incident_authority',
        'incident_service_authority','incident_customer_account_authority',
        'incident_timeline_authority','incident_broadcast_authority',
        'session_authority','session_permission_authority','capability_authority',
        'capability_permission_authority'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_support_write.%I ENABLE ROW LEVEL SECURITY',table_name);
        EXECUTE format('ALTER TABLE erpx_support_write.%I FORCE ROW LEVEL SECURITY',table_name);
        EXECUTE format(
            'CREATE POLICY support_%I_tenant_isolation ON erpx_support_write.%I USING (tenant_id=erpx_cqrs_write.current_tenant_id()) WITH CHECK (tenant_id=erpx_cqrs_write.current_tenant_id())',
            table_name,table_name
        );
    END LOOP;
END $$;
-- END SUPPORT_NORMALIZED_AUTHORITY
