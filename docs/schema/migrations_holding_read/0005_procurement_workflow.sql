-- Disposable Procurement requisition and goods-receipt projections.
-- Holding-write remains the sole command authority.

CREATE TABLE erpx_procurement_read.purchase_requisition_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL,
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    requisition_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    active_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    requisition_code TEXT NOT NULL,
    requesting_department_code TEXT NOT NULL,
    budget_code TEXT,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    requisition_status TEXT NOT NULL
        CHECK (requisition_status IN ('draft','submitted','approved','rejected','cancelled')),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,projection_name,generation,partition_key,requisition_id),
    FOREIGN KEY (holding_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations(holding_id,projection_name,generation)
        ON DELETE CASCADE
);

CREATE INDEX procurement_requisition_projection_scope_status
    ON erpx_procurement_read.purchase_requisition_projection
       (holding_id,projection_name,generation,scope_key,requisition_status,
        aggregate_updated_at DESC,requisition_id DESC);

CREATE TABLE erpx_procurement_read.goods_receipt_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL,
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    goods_receipt_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    active_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    receipt_code TEXT NOT NULL,
    source_purchase_order JSONB NOT NULL,
    receipt_status TEXT NOT NULL
        CHECK (receipt_status IN ('pending_acceptance','accepted','rejected','cancelled')),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,projection_name,generation,partition_key,goods_receipt_id),
    FOREIGN KEY (holding_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations(holding_id,projection_name,generation)
        ON DELETE CASCADE
);

CREATE INDEX procurement_goods_receipt_projection_scope_status
    ON erpx_procurement_read.goods_receipt_projection
       (holding_id,projection_name,generation,scope_key,receipt_status,
        aggregate_updated_at DESC,goods_receipt_id DESC);

DO $$
DECLARE
    projected REGCLASS;
    policy_name TEXT;
BEGIN
    FOREACH projected IN ARRAY ARRAY[
        'erpx_procurement_read.purchase_requisition_projection'::REGCLASS,
        'erpx_procurement_read.goods_receipt_projection'::REGCLASS
    ] LOOP
        EXECUTE format('ALTER TABLE %s ENABLE ROW LEVEL SECURITY',projected);
        EXECUTE format('ALTER TABLE %s FORCE ROW LEVEL SECURITY',projected);
        policy_name := replace(projected::TEXT,'.','_')||'_holding_isolation';
        EXECUTE format(
            'CREATE POLICY %I ON %s USING (holding_id=erpx_cqrs_read.current_holding_id()) WITH CHECK (holding_id=erpx_cqrs_read.current_holding_id())',
            policy_name,projected
        );
    END LOOP;
END;
$$;

UPDATE erpx_meta.schema_manifest
SET installed_at=now()
WHERE singleton=TRUE AND schema_revision=2 AND manifest_version=1
  AND database_role='holding_read' AND migration_set='holding-read';

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM erpx_meta.schema_manifest
        WHERE singleton=TRUE AND schema_revision=2 AND manifest_version=1
          AND database_role='holding_read' AND migration_set='holding-read'
    ) THEN
        RAISE EXCEPTION 'holding-read current manifest is not current';
    END IF;
END;
$$;
