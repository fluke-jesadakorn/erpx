-- Append-only supplier offers; an independently approved award hands off to one ordinary draft PO.
CREATE TABLE procurement_rfqs (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, branch_id TEXT NOT NULL, department_id TEXT,
 warehouse_id TEXT NOT NULL REFERENCES commerce_warehouses(id), currency TEXT NOT NULL,
 title TEXT NOT NULL, demand_evidence TEXT NOT NULL, needed_date TEXT NOT NULL,
 status TEXT NOT NULL CHECK(status IN ('collecting','review','approved','ordered','cancelled')),
 selected_offer_id TEXT, selection_reason TEXT, submitted_by TEXT, approved_by TEXT,
 order_id TEXT UNIQUE REFERENCES commerce_orders(id), created_by TEXT NOT NULL,
 version BIGINT NOT NULL DEFAULT 1 CHECK(version>0), created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
);
CREATE TABLE procurement_rfq_lines (
 id TEXT PRIMARY KEY, rfq_id TEXT NOT NULL REFERENCES procurement_rfqs(id),
 product_id TEXT NOT NULL REFERENCES commerce_products(id), description TEXT NOT NULL, unit TEXT NOT NULL,
 quantity_milli BIGINT NOT NULL CHECK(quantity_milli>0), ordinal INTEGER NOT NULL,
 UNIQUE(rfq_id,product_id), UNIQUE(rfq_id,ordinal)
);
CREATE TABLE procurement_rfq_offers (
 id TEXT PRIMARY KEY, rfq_id TEXT NOT NULL REFERENCES procurement_rfqs(id), supplier_id TEXT NOT NULL REFERENCES finance_parties(id),
 rfq_version BIGINT NOT NULL, reference TEXT NOT NULL, evidence TEXT NOT NULL,
 arrival_date TEXT NOT NULL, valid_until TEXT NOT NULL, currency TEXT NOT NULL,
 subtotal_minor BIGINT NOT NULL CHECK(subtotal_minor>=0), tax_minor BIGINT NOT NULL CHECK(tax_minor>=0),
 recorded_by TEXT NOT NULL, created_at BIGINT NOT NULL, UNIQUE(rfq_id,rfq_version)
);
CREATE TABLE procurement_rfq_offer_lines (
 id TEXT PRIMARY KEY, offer_id TEXT NOT NULL REFERENCES procurement_rfq_offers(id),
 rfq_line_id TEXT NOT NULL REFERENCES procurement_rfq_lines(id), price_minor BIGINT NOT NULL CHECK(price_minor>=0),
 tax_bps BIGINT NOT NULL CHECK(tax_bps BETWEEN 0 AND 10000), subtotal_minor BIGINT NOT NULL, tax_minor BIGINT NOT NULL,
 UNIQUE(offer_id,rfq_line_id)
);
CREATE TABLE procurement_rfq_events (
 id TEXT PRIMARY KEY, rfq_id TEXT NOT NULL REFERENCES procurement_rfqs(id), version BIGINT NOT NULL,
 action TEXT NOT NULL, actor_id TEXT NOT NULL, evidence TEXT NOT NULL, selected_offer_id TEXT,
 created_at BIGINT NOT NULL, UNIQUE(rfq_id,version)
);
ALTER TABLE procurement_rfqs ADD CONSTRAINT procurement_rfq_selected_offer_fk FOREIGN KEY(selected_offer_id) REFERENCES procurement_rfq_offers(id);
CREATE INDEX procurement_rfq_scope ON procurement_rfqs(company_id,branch_id,department_id,status,needed_date);
CREATE INDEX procurement_rfq_supplier_offer ON procurement_rfq_offers(rfq_id,supplier_id,rfq_version DESC);
CREATE FUNCTION procurement_rfq_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'RFQ_EVIDENCE_IMMUTABLE'; END $$;
DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['procurement_rfq_lines','procurement_rfq_offers','procurement_rfq_offer_lines','procurement_rfq_events'] LOOP
 EXECUTE format('CREATE TRIGGER rfq_immutable BEFORE UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION procurement_rfq_immutable()',t);
 END LOOP;
 FOREACH t IN ARRAY ARRAY['procurement_rfqs','procurement_rfq_lines','procurement_rfq_offers','procurement_rfq_offer_lines','procurement_rfq_events'] LOOP
 INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
 EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
 EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
 END LOOP;
END $$;
