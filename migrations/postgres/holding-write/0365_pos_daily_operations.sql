-- Only explicit, enabled register catalogs are sellable. Preserve existing menu opt-ins.
ALTER TABLE pos_catalog ADD COLUMN enabled BOOLEAN NOT NULL DEFAULT true;
CREATE TABLE pos_register_catalog (
 register_id TEXT NOT NULL REFERENCES pos_registers(id) ON DELETE CASCADE,
 product_id TEXT NOT NULL REFERENCES pos_catalog(product_id) ON DELETE CASCADE,
 enabled BOOLEAN NOT NULL DEFAULT true,
 PRIMARY KEY(register_id,product_id)
);
INSERT INTO pos_register_catalog(register_id,product_id)
 SELECT r.id,c.product_id FROM pos_registers r JOIN pos_catalog c ON c.company_id=r.company_id
 WHERE c.kind<>'recipe' OR r.mode='restaurant';
CREATE TABLE pos_product_images (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,product_id TEXT NOT NULL REFERENCES commerce_products(id),
 object_key TEXT NOT NULL UNIQUE,thumbnail_key TEXT NOT NULL UNIQUE,sha256 TEXT NOT NULL,
 width INTEGER NOT NULL CHECK(width>0),height INTEGER NOT NULL CHECK(height>0),
 mime_type TEXT NOT NULL CHECK(mime_type IN ('image/jpeg','image/png','image/webp')),
 source TEXT NOT NULL CHECK(source IN ('generated','upload')),metadata_json JSONB NOT NULL DEFAULT '{}',
 created_by TEXT NOT NULL,created_at BIGINT NOT NULL
);
ALTER TABLE pos_catalog ADD COLUMN image_id TEXT REFERENCES pos_product_images(id);
ALTER TABLE pos_shifts ADD COLUMN business_date TEXT NOT NULL DEFAULT (to_char(current_timestamp AT TIME ZONE 'Asia/Bangkok','YYYY-MM-DD'));
UPDATE pos_shifts SET business_date=to_char(to_timestamp(opened_at/1000.0) AT TIME ZONE 'Asia/Bangkok','YYYY-MM-DD');
ALTER TABLE pos_shifts ADD COLUMN closed_by TEXT;
ALTER TABLE pos_shifts ADD COLUMN denominations_json JSONB;
CREATE TABLE pos_sale_documents (
 sale_id TEXT PRIMARY KEY REFERENCES pos_sales(id),company_id TEXT NOT NULL,branch_id TEXT NOT NULL,
 snapshot_json JSONB NOT NULL,created_at BIGINT NOT NULL
);
CREATE FUNCTION pos_immutable_document() RETURNS trigger LANGUAGE plpgsql AS $$
 BEGIN RAISE EXCEPTION 'POS_DOCUMENT_IMMUTABLE' USING ERRCODE='23514'; END $$;
CREATE TRIGGER pos_receipt_immutable BEFORE UPDATE OR DELETE ON pos_sale_documents
 FOR EACH ROW EXECUTE FUNCTION pos_immutable_document();
CREATE TABLE pos_day_closes (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,branch_id TEXT NOT NULL,business_date TEXT NOT NULL,currency TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('prepared','confirmed')),snapshot_json JSONB NOT NULL,
 source_digest TEXT NOT NULL,prepared_by TEXT NOT NULL,prepared_at BIGINT NOT NULL,
 confirmed_by TEXT,confirmed_at BIGINT,version BIGINT NOT NULL DEFAULT 1,
 UNIQUE(company_id,branch_id,business_date),CHECK(confirmed_by IS NULL OR confirmed_by<>prepared_by)
);
CREATE FUNCTION pos_guard_day_snapshot() RETURNS trigger LANGUAGE plpgsql AS $$
 BEGIN
  IF TG_OP='DELETE' OR OLD.state='confirmed' OR
   (NEW.company_id,NEW.branch_id,NEW.business_date,NEW.currency,NEW.snapshot_json,NEW.source_digest,NEW.prepared_by,NEW.prepared_at)
    IS DISTINCT FROM (OLD.company_id,OLD.branch_id,OLD.business_date,OLD.currency,OLD.snapshot_json,OLD.source_digest,OLD.prepared_by,OLD.prepared_at)
  THEN RAISE EXCEPTION 'POS_DAY_IMMUTABLE' USING ERRCODE='23514'; END IF;
  RETURN NEW;
 END $$;
CREATE TRIGGER pos_day_immutable BEFORE UPDATE OR DELETE ON pos_day_closes
 FOR EACH ROW EXECUTE FUNCTION pos_guard_day_snapshot();
CREATE INDEX pos_shifts_day ON pos_shifts(company_id,branch_id,business_date);
CREATE INDEX pos_sales_day ON pos_sales(company_id,branch_id,business_date);
