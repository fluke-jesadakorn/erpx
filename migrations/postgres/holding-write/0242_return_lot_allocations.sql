-- Preserve the exact fulfilled lot/serial identity through customer and supplier returns.
CREATE TABLE commerce_return_lot_allocations (
 id TEXT PRIMARY KEY,
 return_line_id TEXT NOT NULL REFERENCES commerce_return_lines(id),
 fulfillment_allocation_id TEXT NOT NULL REFERENCES inventory_fulfillment_lot_allocations(id),
 lot_id TEXT NOT NULL REFERENCES inventory_lots(id),
 quantity_milli BIGINT NOT NULL CHECK(quantity_milli>0),
 value_minor BIGINT NOT NULL CHECK(value_minor>=0),
 snapshot_quantity_milli BIGINT NOT NULL CHECK(snapshot_quantity_milli>=0),
 snapshot_value_minor BIGINT NOT NULL CHECK(snapshot_value_minor>=0),
 snapshot_version BIGINT NOT NULL CHECK(snapshot_version>=0),
 UNIQUE(return_line_id,lot_id)
);
CREATE INDEX commerce_return_lot_trace ON commerce_return_lot_allocations(lot_id,return_line_id);
CREATE TRIGGER commerce_return_lot_immutable BEFORE UPDATE OR DELETE ON commerce_return_lot_allocations FOR EACH ROW EXECUTE FUNCTION inventory_lot_evidence_immutable();
INSERT INTO work_cache_revisions(table_name) VALUES('commerce_return_lot_allocations') ON CONFLICT DO NOTHING;
CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON commerce_return_lot_allocations FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON commerce_return_lot_allocations FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed();
