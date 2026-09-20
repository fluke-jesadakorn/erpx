-- Native legal aggregates must not be duplicated into workspace preview records.
CREATE TABLE legal_native_references (
  item_id text NOT NULL REFERENCES legal_items(id),
  target_module text NOT NULL CHECK(target_module IN ('commerce_order','person','employee_request','expense')),
  target_id text NOT NULL,
  ordinal integer NOT NULL CHECK(ordinal BETWEEN 0 AND 49),
  created_by text NOT NULL,
  created_at bigint NOT NULL,
  PRIMARY KEY(item_id,target_module,target_id)
);
CREATE INDEX legal_native_reference_target ON legal_native_references(target_module,target_id);
INSERT INTO work_cache_revisions(table_name) VALUES('legal_native_references');
CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON legal_native_references FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON legal_native_references FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed();
