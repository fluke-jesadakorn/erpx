-- Versioned product inspection specifications and immutable measured results.
CREATE TABLE inventory_quality_specs (
 id TEXT PRIMARY KEY,
 company_id TEXT NOT NULL,
 product_id TEXT NOT NULL REFERENCES commerce_products(id),
 name TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','active','superseded','cancelled')),
 version BIGINT NOT NULL DEFAULT 1 CHECK(version>0),
 created_by TEXT NOT NULL,
 approved_by TEXT,
 approval_reason TEXT,
 created_at BIGINT NOT NULL,
 updated_at BIGINT NOT NULL,
 activated_at BIGINT,
 CHECK(status<>'active' OR (approved_by IS NOT NULL AND approved_by<>created_by AND approval_reason IS NOT NULL AND activated_at IS NOT NULL))
);
CREATE UNIQUE INDEX inventory_quality_active_spec ON inventory_quality_specs(company_id,product_id) WHERE status='active';
CREATE INDEX inventory_quality_spec_product ON inventory_quality_specs(company_id,product_id,status);

CREATE TABLE inventory_quality_spec_metrics (
 id TEXT PRIMARY KEY,
 spec_id TEXT NOT NULL REFERENCES inventory_quality_specs(id),
 code TEXT NOT NULL,
 label TEXT NOT NULL,
 unit TEXT NOT NULL,
 minimum_value NUMERIC,
 maximum_value NUMERIC,
 ordinal BIGINT NOT NULL,
 CHECK(minimum_value IS NOT NULL OR maximum_value IS NOT NULL),
 CHECK(minimum_value IS NULL OR maximum_value IS NULL OR minimum_value<=maximum_value),
 UNIQUE(spec_id,code),
 UNIQUE(spec_id,ordinal)
);

ALTER TABLE inventory_quality_holds ADD COLUMN spec_id TEXT REFERENCES inventory_quality_specs(id);
ALTER TABLE inventory_quality_holds ADD COLUMN spec_version BIGINT;
ALTER TABLE inventory_quality_inspections ADD COLUMN spec_id TEXT REFERENCES inventory_quality_specs(id);
ALTER TABLE inventory_quality_inspections ADD COLUMN spec_version BIGINT;

CREATE TABLE inventory_quality_measurements (
 id TEXT PRIMARY KEY,
 inspection_id TEXT NOT NULL REFERENCES inventory_quality_inspections(id),
 metric_id TEXT NOT NULL REFERENCES inventory_quality_spec_metrics(id),
 code TEXT NOT NULL,
 label TEXT NOT NULL,
 unit TEXT NOT NULL,
 minimum_value NUMERIC,
 maximum_value NUMERIC,
 measured_value NUMERIC NOT NULL,
 passed BIGINT NOT NULL CHECK(passed IN (0,1)),
 ordinal BIGINT NOT NULL,
 UNIQUE(inspection_id,metric_id),
 UNIQUE(inspection_id,ordinal)
);

CREATE TABLE inventory_quality_inspection_attachments (
 inspection_id TEXT NOT NULL REFERENCES inventory_quality_inspections(id),
 attachment_id TEXT NOT NULL UNIQUE REFERENCES attachments(id),
 ordinal BIGINT NOT NULL,
 PRIMARY KEY(inspection_id,attachment_id),
 UNIQUE(inspection_id,ordinal)
);

CREATE FUNCTION quality_spec_metric_edit_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE state TEXT;
BEGIN
 SELECT status INTO state FROM inventory_quality_specs WHERE id=COALESCE(NEW.spec_id,OLD.spec_id);
 IF state<>'draft' THEN RAISE EXCEPTION 'QUALITY_SPEC_IMMUTABLE' USING ERRCODE='23514'; END IF;
 RETURN COALESCE(NEW,OLD);
END $$;
CREATE TRIGGER quality_spec_metric_edit_guard BEFORE INSERT OR UPDATE OR DELETE ON inventory_quality_spec_metrics FOR EACH ROW EXECUTE FUNCTION quality_spec_metric_edit_guard();

CREATE FUNCTION quality_measurement_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'QUALITY_MEASUREMENT_IMMUTABLE' USING ERRCODE='23514'; END $$;
CREATE TRIGGER quality_measurement_immutable BEFORE UPDATE OR DELETE ON inventory_quality_measurements FOR EACH ROW EXECUTE FUNCTION quality_measurement_immutable();
CREATE TRIGGER quality_inspection_attachment_immutable BEFORE UPDATE OR DELETE ON inventory_quality_inspection_attachments FOR EACH ROW EXECUTE FUNCTION quality_measurement_immutable();

CREATE FUNCTION quality_spec_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF OLD.status IN ('active','superseded','cancelled') AND ROW(NEW.*) IS DISTINCT FROM ROW(OLD.*) AND NOT (OLD.status='active' AND NEW.status='superseded' AND NEW.version=OLD.version+1) THEN
  RAISE EXCEPTION 'QUALITY_SPEC_IMMUTABLE' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER quality_spec_guard BEFORE UPDATE ON inventory_quality_specs FOR EACH ROW EXECUTE FUNCTION quality_spec_guard();

DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['inventory_quality_specs','inventory_quality_spec_metrics','inventory_quality_measurements','inventory_quality_inspection_attachments'] LOOP
  INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
  EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
  EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
 END LOOP;
END $$;
