-- Wave 6 paired-context relational guards. Migration 0081 promotes these
-- roots to typed authority while receipts, audit and outbox facts remain
-- atomic through erpx_cqrs_write.

CREATE SCHEMA IF NOT EXISTS erpx_marketing_write;
CREATE TABLE erpx_marketing_write.lead_candidate_guards (
    lead_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    campaign_id UUID NOT NULL,
    audience_member_id UUID NOT NULL,
    lead_status TEXT NOT NULL CHECK (lead_status IN ('open', 'qualified', 'rejected')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX marketing_lead_scope_status
    ON erpx_marketing_write.lead_candidate_guards
        (scope_key, lead_status, updated_at DESC, lead_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_crm_write;
CREATE TABLE erpx_crm_write.lead_guards (
    lead_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    marketing_candidate_id UUID NOT NULL UNIQUE,
    customer_id UUID,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    accepted_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE erpx_crm_write.opportunity_guards (
    opportunity_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    accepted_lead_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    opportunity_stage TEXT NOT NULL CHECK (opportunity_stage IN (
        'qualified', 'discovery', 'proposal', 'negotiation', 'won', 'lost'
    )),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    FOREIGN KEY (accepted_lead_id)
        REFERENCES erpx_crm_write.lead_guards (lead_id)
);
CREATE INDEX crm_opportunity_scope_customer_stage
    ON erpx_crm_write.opportunity_guards
        (scope_key, customer_id, opportunity_stage, updated_at DESC, opportunity_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_procurement_write;
CREATE TABLE erpx_procurement_write.purchase_order_guards (
    purchase_order_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    order_code TEXT NOT NULL,
    supplier_id UUID NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    order_status TEXT NOT NULL CHECK (order_status IN ('draft', 'released', 'cancelled')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (scope_key, order_code)
);
CREATE INDEX procurement_order_scope_supplier_status
    ON erpx_procurement_write.purchase_order_guards
        (scope_key, supplier_id, order_status, updated_at DESC, purchase_order_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_inventory_write;
CREATE TABLE erpx_inventory_write.stock_item_guards (
    stock_item_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    warehouse_id UUID NOT NULL,
    item_code TEXT NOT NULL,
    on_hand BIGINT NOT NULL CHECK (on_hand >= 0),
    reserved BIGINT NOT NULL CHECK (reserved >= 0 AND reserved <= on_hand),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (scope_key, warehouse_id, item_code)
);
CREATE TABLE erpx_inventory_write.movement_guards (
    movement_id UUID PRIMARY KEY,
    stock_item_id UUID NOT NULL,
    quantity_delta BIGINT NOT NULL CHECK (quantity_delta <> 0),
    procurement_line_id UUID,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    occurred_at TIMESTAMPTZ NOT NULL,
    FOREIGN KEY (stock_item_id)
        REFERENCES erpx_inventory_write.stock_item_guards (stock_item_id)
        ON DELETE CASCADE
);
CREATE TABLE erpx_inventory_write.reservation_guards (
    reservation_id UUID PRIMARY KEY,
    stock_item_id UUID NOT NULL,
    quantity BIGINT NOT NULL CHECK (quantity > 0),
    demand_reference TEXT NOT NULL,
    reservation_status TEXT NOT NULL CHECK (reservation_status IN ('active', 'released')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    FOREIGN KEY (stock_item_id)
        REFERENCES erpx_inventory_write.stock_item_guards (stock_item_id)
        ON DELETE CASCADE
);
CREATE INDEX inventory_stock_scope_warehouse_code
    ON erpx_inventory_write.stock_item_guards
        (scope_key, warehouse_id, item_code, updated_at DESC, stock_item_id DESC);
CREATE INDEX inventory_active_reservations
    ON erpx_inventory_write.reservation_guards (stock_item_id, reservation_id)
    WHERE reservation_status = 'active';

CREATE SCHEMA IF NOT EXISTS erpx_assets_write;
CREATE TABLE erpx_assets_write.asset_guards (
    asset_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    asset_code TEXT NOT NULL,
    location_id UUID NOT NULL,
    asset_status TEXT NOT NULL CHECK (asset_status IN (
        'draft', 'in_service', 'under_maintenance', 'retired'
    )),
    asset_condition TEXT NOT NULL CHECK (asset_condition IN (
        'operational', 'degraded', 'out_of_service', 'retired'
    )),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (scope_key, asset_code)
);
CREATE INDEX assets_scope_location_status
    ON erpx_assets_write.asset_guards
        (scope_key, location_id, asset_status, updated_at DESC, asset_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_maintenance_write;
CREATE TABLE erpx_maintenance_write.asset_fact_inbox (
    asset_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_event_id UUID NOT NULL UNIQUE,
    tenant_scope JSONB NOT NULL,
    asset_code TEXT NOT NULL,
    asset_name TEXT NOT NULL,
    location_id UUID NOT NULL,
    asset_condition TEXT NOT NULL CHECK (asset_condition IN (
        'operational', 'degraded', 'out_of_service', 'retired'
    )),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (asset_id, source_aggregate_version)
);
CREATE TABLE erpx_maintenance_write.work_order_guards (
    work_order_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    work_order_code TEXT NOT NULL,
    asset_id UUID NOT NULL,
    request_code TEXT NOT NULL,
    source_event_id UUID NOT NULL UNIQUE,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    work_order_status TEXT NOT NULL CHECK (work_order_status IN (
        'planned', 'scheduled', 'in_progress', 'blocked', 'completed', 'cancelled'
    )),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (scope_key, work_order_code),
    FOREIGN KEY (asset_id, source_aggregate_version)
        REFERENCES erpx_maintenance_write.asset_fact_inbox
            (asset_id, source_aggregate_version)
);
CREATE TABLE erpx_maintenance_write.downtime_guards (
    downtime_id UUID PRIMARY KEY,
    work_order_id UUID NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    ended_at TIMESTAMPTZ NOT NULL,
    reason TEXT NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    CHECK (ended_at > started_at),
    FOREIGN KEY (work_order_id)
        REFERENCES erpx_maintenance_write.work_order_guards (work_order_id)
        ON DELETE CASCADE
);
CREATE INDEX maintenance_scope_asset_status
    ON erpx_maintenance_write.work_order_guards
        (scope_key, asset_id, work_order_status, updated_at DESC, work_order_id DESC);
