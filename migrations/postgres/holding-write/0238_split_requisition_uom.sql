-- One approved requisition can be sourced through several RFQs and supplier awards.
-- Each immutable RFQ line snapshots both the requested unit and the purchasing unit.
DROP INDEX IF EXISTS procurement_rfq_active_request;

ALTER TABLE procurement_rfq_lines
  ADD COLUMN request_quantity_milli BIGINT CHECK(request_quantity_milli>0),
  ADD COLUMN request_unit TEXT,
  ADD COLUMN request_units_per_product_unit_milli BIGINT CHECK(request_units_per_product_unit_milli>0);

DROP TRIGGER rfq_immutable ON procurement_rfq_lines;
UPDATE procurement_rfq_lines l
SET request_quantity_milli=l.quantity_milli,
    request_unit=COALESCE(NULLIF(r.request_items_json::jsonb -> l.request_item_ordinal ->> 'unit',''),'each'),
    request_units_per_product_unit_milli=1000
FROM procurement_rfqs r
WHERE r.id=l.rfq_id AND r.request_id IS NOT NULL;
CREATE TRIGGER rfq_immutable BEFORE UPDATE OR DELETE ON procurement_rfq_lines FOR EACH ROW EXECUTE FUNCTION procurement_rfq_immutable();

ALTER TABLE procurement_rfq_lines ADD CONSTRAINT procurement_rfq_request_mapping_complete CHECK(
  (request_item_ordinal IS NULL AND request_quantity_milli IS NULL AND request_unit IS NULL AND request_units_per_product_unit_milli IS NULL)
  OR
  (request_item_ordinal IS NOT NULL AND request_quantity_milli IS NOT NULL AND request_unit IS NOT NULL AND request_units_per_product_unit_milli IS NOT NULL
   AND request_quantity_milli::numeric*1000=quantity_milli::numeric*request_units_per_product_unit_milli::numeric)
);

CREATE INDEX procurement_rfq_request_allocations
  ON procurement_rfqs(request_id,status) WHERE request_id IS NOT NULL;
CREATE UNIQUE INDEX procurement_rfq_request_item_once_per_rfq
  ON procurement_rfq_lines(rfq_id,request_item_ordinal) WHERE request_item_ordinal IS NOT NULL;

CREATE TABLE procurement_request_orders (
  request_id TEXT NOT NULL REFERENCES employee_requests(id),
  order_id TEXT NOT NULL UNIQUE REFERENCES commerce_orders(id),
  rfq_id TEXT REFERENCES procurement_rfqs(id),
  request_version BIGINT NOT NULL CHECK(request_version>0),
  request_evidence_version BIGINT NOT NULL CHECK(request_evidence_version>0),
  linked_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  PRIMARY KEY(request_id,order_id),
  UNIQUE(rfq_id)
);
CREATE INDEX procurement_request_orders_request ON procurement_request_orders(request_id,created_at);

INSERT INTO procurement_request_orders(request_id,order_id,rfq_id,request_version,request_evidence_version,linked_by,created_at)
SELECT r.request_id,r.order_id,r.id,COALESCE(r.request_version,1),COALESCE(r.request_evidence_version,1),r.created_by,r.updated_at
FROM procurement_rfqs r
WHERE r.request_id IS NOT NULL AND r.order_id IS NOT NULL
ON CONFLICT DO NOTHING;

-- A locked counter makes concurrent allocation safe under PostgreSQL READ COMMITTED.
CREATE TABLE procurement_request_item_commitments (
  request_id TEXT NOT NULL REFERENCES employee_requests(id),
  request_item_ordinal INTEGER NOT NULL CHECK(request_item_ordinal>=0),
  request_evidence_version BIGINT NOT NULL CHECK(request_evidence_version>0),
  approved_quantity_milli BIGINT NOT NULL CHECK(approved_quantity_milli>0),
  allocated_quantity_milli BIGINT NOT NULL DEFAULT 0 CHECK(allocated_quantity_milli BETWEEN 0 AND approved_quantity_milli),
  PRIMARY KEY(request_id,request_item_ordinal)
);
INSERT INTO procurement_request_item_commitments(request_id,request_item_ordinal,request_evidence_version,approved_quantity_milli,allocated_quantity_milli)
SELECT r.request_id,l.request_item_ordinal,MAX(r.request_evidence_version),
       round(MAX(NULLIF(r.request_items_json::jsonb -> l.request_item_ordinal ->> 'quantity','')::numeric)*1000),
       sum(l.request_quantity_milli)
FROM procurement_rfq_lines l JOIN procurement_rfqs r ON r.id=l.rfq_id
WHERE r.request_id IS NOT NULL AND r.status<>'cancelled'
GROUP BY r.request_id,l.request_item_ordinal;

CREATE FUNCTION procurement_validate_request_allocation() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
  source_request_id TEXT;
  source_items JSONB;
  source_evidence_version BIGINT;
  approved_milli BIGINT;
  changed_rows INTEGER;
BEGIN
  SELECT r.request_id,r.request_items_json::jsonb,r.request_evidence_version
  INTO source_request_id,source_items,source_evidence_version
  FROM procurement_rfqs r WHERE r.id=NEW.rfq_id;
  IF source_request_id IS NULL THEN RETURN NEW; END IF;
  IF NEW.request_item_ordinal IS NULL OR NEW.request_quantity_milli IS NULL THEN
    RAISE EXCEPTION 'RFQ_REQUEST_MAPPING_REQUIRED';
  END IF;
  approved_milli=round(COALESCE(NULLIF(source_items -> NEW.request_item_ordinal ->> 'quantity',''),'0')::numeric*1000);
  IF approved_milli<=0 THEN RAISE EXCEPTION 'RFQ_REQUEST_MAPPING_REQUIRED'; END IF;
  INSERT INTO procurement_request_item_commitments(request_id,request_item_ordinal,request_evidence_version,approved_quantity_milli,allocated_quantity_milli)
  VALUES(source_request_id,NEW.request_item_ordinal,source_evidence_version,approved_milli,0)
  ON CONFLICT DO NOTHING;
  UPDATE procurement_request_item_commitments
  SET allocated_quantity_milli=allocated_quantity_milli+NEW.request_quantity_milli
  WHERE request_id=source_request_id AND request_item_ordinal=NEW.request_item_ordinal
    AND request_evidence_version=source_evidence_version AND approved_quantity_milli=approved_milli
    AND allocated_quantity_milli+NEW.request_quantity_milli<=approved_milli;
  GET DIAGNOSTICS changed_rows=ROW_COUNT;
  IF changed_rows<>1 THEN
    RAISE EXCEPTION 'RFQ_REQUEST_OVERALLOCATED';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER procurement_request_allocation_guard
  BEFORE INSERT ON procurement_rfq_lines FOR EACH ROW
  EXECUTE FUNCTION procurement_validate_request_allocation();

CREATE FUNCTION procurement_release_request_allocation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.status<>'cancelled' AND NEW.status='cancelled' AND OLD.request_id IS NOT NULL THEN
    UPDATE procurement_request_item_commitments c
    SET allocated_quantity_milli=c.allocated_quantity_milli-x.quantity_milli
    FROM (SELECT request_item_ordinal,sum(request_quantity_milli) quantity_milli FROM procurement_rfq_lines WHERE rfq_id=OLD.id GROUP BY request_item_ordinal) x
    WHERE c.request_id=OLD.request_id AND c.request_item_ordinal=x.request_item_ordinal;
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER procurement_release_request_allocation
  AFTER UPDATE OF status ON procurement_rfqs FOR EACH ROW
  EXECUTE FUNCTION procurement_release_request_allocation();

CREATE FUNCTION procurement_request_order_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'REQUEST_ORDER_LINK_IMMUTABLE'; END $$;
CREATE TRIGGER procurement_request_order_immutable
  BEFORE UPDATE OR DELETE ON procurement_request_orders FOR EACH ROW
  EXECUTE FUNCTION procurement_request_order_immutable();

INSERT INTO work_cache_revisions(table_name) VALUES('procurement_request_orders') ON CONFLICT DO NOTHING;
CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON procurement_request_orders FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON procurement_request_orders FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed();
