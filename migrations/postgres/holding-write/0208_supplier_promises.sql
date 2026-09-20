-- Supplier confirmations supplement an approved PO; they do not move stock or change its required date.
CREATE TABLE procurement_delivery_promises (
 id TEXT PRIMARY KEY, order_id TEXT NOT NULL REFERENCES commerce_orders(id), company_id TEXT NOT NULL,
 order_version BIGINT NOT NULL, supplier_id TEXT NOT NULL, currency TEXT NOT NULL, required_date TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('confirmed','withdrawn')), confirmation_date TEXT NOT NULL,
 supplier_reference TEXT NOT NULL, evidence TEXT NOT NULL, recorded_by TEXT NOT NULL, created_at BIGINT NOT NULL,
 UNIQUE(order_id,order_version)
);
CREATE TABLE procurement_delivery_promise_lines (
 id TEXT PRIMARY KEY, promise_id TEXT NOT NULL REFERENCES procurement_delivery_promises(id),
 order_line_id TEXT NOT NULL REFERENCES commerce_order_lines(id), ordinal INTEGER NOT NULL,
 quantity_milli BIGINT NOT NULL CHECK(quantity_milli>0), fulfilled_snapshot_milli BIGINT NOT NULL CHECK(fulfilled_snapshot_milli>=0),
 promised_date TEXT NOT NULL, UNIQUE(promise_id,ordinal), UNIQUE(promise_id,order_line_id,promised_date)
);
CREATE INDEX procurement_promises_order ON procurement_delivery_promises(order_id,order_version DESC);
CREATE FUNCTION procurement_promise_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'SUPPLIER_PROMISE_IMMUTABLE'; END $$;
CREATE TRIGGER procurement_promise_immutable BEFORE UPDATE OR DELETE ON procurement_delivery_promises FOR EACH ROW EXECUTE FUNCTION procurement_promise_immutable();
CREATE TRIGGER procurement_promise_line_immutable BEFORE UPDATE OR DELETE ON procurement_delivery_promise_lines FOR EACH ROW EXECUTE FUNCTION procurement_promise_immutable();
-- Actual receipts remain the sole source of fulfilled quantity. Allocate them earliest promised date first.
CREATE VIEW procurement_open_promise_lines AS
 WITH latest AS (SELECT DISTINCT ON(order_id) id,order_id,kind FROM procurement_delivery_promises ORDER BY order_id,order_version DESC),
 schedule AS (SELECT p.*,o.company_id,o.branch_id,o.department_id,o.number,o.state,l.product_id,l.description,l.fulfilled_milli,
 COALESCE(SUM(p.quantity_milli) OVER(PARTITION BY p.promise_id,p.order_line_id ORDER BY p.promised_date,p.ordinal ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING),0) prior_quantity
 FROM latest h JOIN procurement_delivery_promise_lines p ON p.promise_id=h.id JOIN commerce_order_lines l ON l.id=p.order_line_id
 JOIN commerce_orders o ON o.id=h.order_id WHERE h.kind='confirmed' AND o.state IN ('confirmed','partially_fulfilled'))
 SELECT schedule.*,GREATEST(0,quantity_milli-GREATEST(0,fulfilled_milli-fulfilled_snapshot_milli-prior_quantity))::bigint remaining_milli FROM schedule;
DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['procurement_delivery_promises','procurement_delivery_promise_lines'] LOOP
 INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
 EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
 EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
 END LOOP;
END $$;
