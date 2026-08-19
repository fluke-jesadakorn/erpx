-- Version-neutral Quality write contract. Command receipts, audit evidence,
-- and outbox facts are provided by erpx_cqrs_write; aggregate authority is
-- the typed relational row below.
CREATE SCHEMA IF NOT EXISTS erpx_quality_write;

CREATE TABLE IF NOT EXISTS erpx_quality_write.inspection_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
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
    PRIMARY KEY (holding_id, inspection_id),
    UNIQUE (holding_id, inspection_id, aggregate_version),
    UNIQUE NULLS NOT DISTINCT (
        holding_id, holding_id, company_id, branch_id, reference
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

CREATE INDEX IF NOT EXISTS quality_inspection_authority_scope
    ON erpx_quality_write.inspection_authority
        (holding_id, holding_id, company_id, branch_id, inspection_status,
         updated_at DESC, inspection_id DESC);

ALTER TABLE erpx_quality_write.inspection_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_quality_write.inspection_authority FORCE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS quality_inspection_authority_holding_isolation
    ON erpx_quality_write.inspection_authority;
CREATE POLICY quality_inspection_authority_holding_isolation
    ON erpx_quality_write.inspection_authority
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());
