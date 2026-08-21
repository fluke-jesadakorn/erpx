-- Phase 5: typed Procurement workflow records and exact PO source evidence.
-- Historical purchase orders may retain NULL source evidence for bounded read
-- compatibility. New owner commands require a requisition and cannot release
-- a legacy-unlinked order.

ALTER TABLE erpx_procurement_write.purchase_order_authority
    ADD COLUMN source_requisition JSONB,
    ADD COLUMN contract_reference JSONB;

CREATE TABLE erpx_procurement_write.purchase_requisition_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    requisition_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    requisition_code TEXT NOT NULL CHECK (octet_length(requisition_code) BETWEEN 1 AND 128),
    requesting_department_code TEXT NOT NULL
        CHECK (octet_length(requesting_department_code) BETWEEN 1 AND 128),
    budget_code TEXT NOT NULL CHECK (octet_length(budget_code) BETWEEN 1 AND 128),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    requisition_status TEXT NOT NULL
        CHECK (requisition_status IN ('draft','submitted','approved','rejected','cancelled')),
    state JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,requisition_id),
    UNIQUE (holding_id,scope_key,requisition_code),
    CHECK (updated_at >= created_at)
);

CREATE TABLE erpx_procurement_write.purchase_requisition_line_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    requisition_id UUID NOT NULL,
    line_id UUID NOT NULL,
    line_position INTEGER NOT NULL CHECK (line_position >= 0 AND line_position < 10000),
    item_code TEXT NOT NULL CHECK (octet_length(item_code) BETWEEN 1 AND 128),
    requested_quantity BIGINT NOT NULL CHECK (requested_quantity > 0),
    estimated_unit_price_minor BIGINT NOT NULL CHECK (estimated_unit_price_minor > 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    destination_code TEXT NOT NULL CHECK (octet_length(destination_code) BETWEEN 1 AND 128),
    PRIMARY KEY (holding_id,requisition_id,line_id),
    UNIQUE (holding_id,requisition_id,line_position),
    FOREIGN KEY (holding_id,requisition_id)
        REFERENCES erpx_procurement_write.purchase_requisition_authority(holding_id,requisition_id)
        ON DELETE CASCADE
);

CREATE TABLE erpx_procurement_write.goods_receipt_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    goods_receipt_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    receipt_code TEXT NOT NULL CHECK (octet_length(receipt_code) BETWEEN 1 AND 128),
    source_purchase_order JSONB NOT NULL,
    receipt_status TEXT NOT NULL
        CHECK (receipt_status IN ('pending_acceptance','accepted','rejected','cancelled')),
    state JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,goods_receipt_id),
    UNIQUE (holding_id,scope_key,receipt_code),
    CHECK (updated_at >= created_at)
);

CREATE TABLE erpx_procurement_write.goods_receipt_line_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    goods_receipt_id UUID NOT NULL,
    line_id UUID NOT NULL,
    line_position INTEGER NOT NULL CHECK (line_position >= 0 AND line_position < 10000),
    purchase_order_line_id UUID NOT NULL,
    inventory_stock_item_id UUID NOT NULL,
    received_quantity BIGINT NOT NULL CHECK (received_quantity > 0),
    PRIMARY KEY (holding_id,goods_receipt_id,line_id),
    UNIQUE (holding_id,goods_receipt_id,line_position),
    FOREIGN KEY (holding_id,goods_receipt_id)
        REFERENCES erpx_procurement_write.goods_receipt_authority(holding_id,goods_receipt_id)
        ON DELETE CASCADE
);

CREATE INDEX purchase_requisition_authority_scope_status
    ON erpx_procurement_write.purchase_requisition_authority
        (holding_id,company_id,branch_id,requisition_status,updated_at DESC);
CREATE INDEX goods_receipt_authority_scope_status
    ON erpx_procurement_write.goods_receipt_authority
        (holding_id,company_id,branch_id,receipt_status,updated_at DESC);

ALTER TABLE erpx_procurement_write.purchase_requisition_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_procurement_write.purchase_requisition_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY purchase_requisition_holding_isolation
    ON erpx_procurement_write.purchase_requisition_authority
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());
ALTER TABLE erpx_procurement_write.purchase_requisition_line_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_procurement_write.purchase_requisition_line_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY purchase_requisition_line_holding_isolation
    ON erpx_procurement_write.purchase_requisition_line_authority
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());
ALTER TABLE erpx_procurement_write.goods_receipt_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_procurement_write.goods_receipt_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY goods_receipt_holding_isolation
    ON erpx_procurement_write.goods_receipt_authority
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());
ALTER TABLE erpx_procurement_write.goods_receipt_line_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_procurement_write.goods_receipt_line_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY goods_receipt_line_holding_isolation
    ON erpx_procurement_write.goods_receipt_line_authority
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());
