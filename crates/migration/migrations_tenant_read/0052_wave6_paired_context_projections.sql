-- Disposable Wave 6 projections. Generation-scoped rows permit shadow
-- rebuilds and atomic activation while command-side writes continue.

CREATE SCHEMA IF NOT EXISTS erpx_marketing_read;
CREATE TABLE erpx_marketing_read.lead_candidate_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'marketing.lead_candidates'
        CHECK (projection_name = 'marketing.lead_candidates'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    lead_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    campaign_id UUID NOT NULL,
    audience_member_id UUID NOT NULL,
    source TEXT NOT NULL,
    existing_customer_id UUID,
    score_basis_points INTEGER CHECK (score_basis_points BETWEEN 0 AND 10000),
    lead_status TEXT NOT NULL CHECK (lead_status IN ('open', 'qualified', 'rejected')),
    engagement_count BIGINT NOT NULL CHECK (engagement_count >= 0),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, lead_id),
    FOREIGN KEY (tenant_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (tenant_id, projection_name, generation)
        ON DELETE CASCADE
);
CREATE INDEX marketing_read_scope_status
    ON erpx_marketing_read.lead_candidate_projection
        (tenant_id, generation, scope_key, lead_status, aggregate_updated_at DESC, lead_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_crm_read;
CREATE TABLE erpx_crm_read.lead_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'crm.leads'
        CHECK (projection_name = 'crm.leads'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    lead_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    marketing_candidate_id UUID NOT NULL,
    customer_id UUID,
    source TEXT NOT NULL,
    qualification_score_basis_points INTEGER NOT NULL
        CHECK (qualification_score_basis_points BETWEEN 0 AND 10000),
    accepted_at TIMESTAMPTZ NOT NULL,
    state JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, lead_id),
    FOREIGN KEY (tenant_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (tenant_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (tenant_id, generation, marketing_candidate_id)
);
CREATE TABLE erpx_crm_read.opportunity_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'crm.opportunities'
        CHECK (projection_name = 'crm.opportunities'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    opportunity_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    accepted_lead_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    name TEXT NOT NULL,
    expected_value_minor BIGINT NOT NULL CHECK (expected_value_minor >= 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    opportunity_stage TEXT NOT NULL CHECK (opportunity_stage IN (
        'qualified', 'discovery', 'proposal', 'negotiation', 'won', 'lost'
    )),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, opportunity_id),
    FOREIGN KEY (tenant_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (tenant_id, projection_name, generation)
        ON DELETE CASCADE
);
CREATE INDEX crm_read_opportunity_scope_customer_stage
    ON erpx_crm_read.opportunity_projection
        (tenant_id, generation, scope_key, customer_id, opportunity_stage,
         aggregate_updated_at DESC, opportunity_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_procurement_read;
CREATE TABLE erpx_procurement_read.purchase_order_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'procurement.purchase_orders'
        CHECK (projection_name = 'procurement.purchase_orders'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    purchase_order_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    order_code TEXT NOT NULL,
    supplier_id UUID NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    order_status TEXT NOT NULL CHECK (order_status IN ('draft', 'released', 'cancelled')),
    lines JSONB NOT NULL,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, purchase_order_id),
    FOREIGN KEY (tenant_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (tenant_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (tenant_id, generation, scope_key, order_code)
);
CREATE INDEX procurement_read_scope_supplier_status
    ON erpx_procurement_read.purchase_order_projection
        (tenant_id, generation, scope_key, supplier_id, order_status,
         aggregate_updated_at DESC, purchase_order_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_inventory_read;
CREATE TABLE erpx_inventory_read.stock_item_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'inventory.stock_items'
        CHECK (projection_name = 'inventory.stock_items'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    stock_item_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    warehouse_id UUID NOT NULL,
    item_code TEXT NOT NULL,
    on_hand BIGINT NOT NULL CHECK (on_hand >= 0),
    reserved BIGINT NOT NULL CHECK (reserved >= 0 AND reserved <= on_hand),
    available BIGINT NOT NULL CHECK (available = on_hand - reserved),
    reservations JSONB NOT NULL,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, stock_item_id),
    FOREIGN KEY (tenant_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (tenant_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (tenant_id, generation, scope_key, warehouse_id, item_code)
);
CREATE INDEX inventory_read_scope_warehouse_code
    ON erpx_inventory_read.stock_item_projection
        (tenant_id, generation, scope_key, warehouse_id, item_code,
         aggregate_updated_at DESC, stock_item_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_assets_read;
CREATE TABLE erpx_assets_read.asset_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'assets.assets'
        CHECK (projection_name = 'assets.assets'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    asset_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    asset_code TEXT NOT NULL,
    asset_name TEXT NOT NULL,
    location_id UUID NOT NULL,
    asset_status TEXT NOT NULL CHECK (asset_status IN (
        'draft', 'in_service', 'under_maintenance', 'retired'
    )),
    asset_condition TEXT NOT NULL CHECK (asset_condition IN (
        'operational', 'degraded', 'out_of_service', 'retired'
    )),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, asset_id),
    FOREIGN KEY (tenant_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (tenant_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (tenant_id, generation, scope_key, asset_code)
);
CREATE INDEX assets_read_scope_location_status
    ON erpx_assets_read.asset_projection
        (tenant_id, generation, scope_key, location_id, asset_status,
         aggregate_updated_at DESC, asset_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_maintenance_read;
CREATE TABLE erpx_maintenance_read.work_order_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'maintenance.work_orders'
        CHECK (projection_name = 'maintenance.work_orders'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    work_order_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    work_order_code TEXT NOT NULL,
    asset_id UUID NOT NULL,
    request_code TEXT NOT NULL,
    source_event_id UUID NOT NULL,
    source_asset_version BIGINT NOT NULL CHECK (source_asset_version > 0),
    title TEXT NOT NULL,
    work_order_status TEXT NOT NULL CHECK (work_order_status IN (
        'planned', 'scheduled', 'in_progress', 'blocked', 'completed', 'cancelled'
    )),
    scheduled_for TIMESTAMPTZ,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, work_order_id),
    FOREIGN KEY (tenant_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (tenant_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (tenant_id, generation, source_event_id),
    UNIQUE (tenant_id, generation, scope_key, work_order_code)
);
CREATE INDEX maintenance_read_scope_asset_status
    ON erpx_maintenance_read.work_order_projection
        (tenant_id, generation, scope_key, asset_id, work_order_status,
         aggregate_updated_at DESC, work_order_id DESC);

DO $$
DECLARE schema_name TEXT; table_name TEXT;
BEGIN
    FOR schema_name, table_name IN SELECT * FROM (VALUES
        ('erpx_marketing_read','lead_candidate_projection'),
        ('erpx_crm_read','lead_projection'),
        ('erpx_crm_read','opportunity_projection'),
        ('erpx_procurement_read','purchase_order_projection'),
        ('erpx_inventory_read','stock_item_projection'),
        ('erpx_assets_read','asset_projection'),
        ('erpx_maintenance_read','work_order_projection')
    ) AS protected(schema_name, table_name)
    LOOP
        EXECUTE format('ALTER TABLE %I.%I ENABLE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format('ALTER TABLE %I.%I FORCE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format(
            'CREATE POLICY tenant_isolation ON %I.%I '
            'USING (tenant_id = erpx_cqrs_read.current_tenant_id()) '
            'WITH CHECK (tenant_id = erpx_cqrs_read.current_tenant_id())',
            schema_name, table_name
        );
    END LOOP;
END;
$$;
