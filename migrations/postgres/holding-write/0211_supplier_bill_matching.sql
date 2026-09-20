CREATE TABLE procurement_bill_matches (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, branch_id TEXT NOT NULL, department_id TEXT,
 order_id TEXT NOT NULL REFERENCES commerce_orders(id), fulfillment_id TEXT NOT NULL REFERENCES commerce_fulfillments(id),
 supplier_id TEXT NOT NULL REFERENCES finance_parties(id), currency TEXT NOT NULL, chart_id TEXT NOT NULL,
 clearing_account_id TEXT NOT NULL, payable_account_id TEXT NOT NULL, tax_account_id TEXT NOT NULL,
 reference TEXT NOT NULL, invoice_date TEXT NOT NULL, due_date TEXT NOT NULL, evidence TEXT NOT NULL,
 expected_subtotal_minor BIGINT NOT NULL, subtotal_minor BIGINT NOT NULL, tax_minor BIGINT NOT NULL,
 quantity_matches BOOLEAN NOT NULL, variance_account_id TEXT, reviewed_by TEXT, review_reason TEXT,
 invoice_id TEXT UNIQUE REFERENCES finance_invoices(id),
 status TEXT NOT NULL CHECK(status IN ('review','returned','approved','invoiced','posted')),
 version BIGINT NOT NULL DEFAULT 1 CHECK(version>0), created_by TEXT NOT NULL, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
);
CREATE UNIQUE INDEX procurement_bill_active_receipt ON procurement_bill_matches(fulfillment_id) WHERE status<>'returned';
CREATE UNIQUE INDEX procurement_bill_active_reference ON procurement_bill_matches(company_id,supplier_id,reference) WHERE status<>'returned';
CREATE TABLE procurement_bill_match_lines (
 id TEXT PRIMARY KEY, match_id TEXT NOT NULL REFERENCES procurement_bill_matches(id),
 fulfillment_line_id TEXT NOT NULL REFERENCES commerce_fulfillment_lines(id),
 quantity_milli BIGINT NOT NULL CHECK(quantity_milli>0), expected_quantity_milli BIGINT NOT NULL CHECK(expected_quantity_milli>0),
 price_minor BIGINT NOT NULL CHECK(price_minor>=0), tax_bps BIGINT NOT NULL CHECK(tax_bps BETWEEN 0 AND 10000),
 expected_subtotal_minor BIGINT NOT NULL, subtotal_minor BIGINT NOT NULL, tax_minor BIGINT NOT NULL,
 UNIQUE(match_id,fulfillment_line_id)
);
CREATE TABLE procurement_bill_match_events (
 id TEXT PRIMARY KEY, match_id TEXT NOT NULL REFERENCES procurement_bill_matches(id), version BIGINT NOT NULL,
 action TEXT NOT NULL, actor_id TEXT NOT NULL, reason TEXT NOT NULL, created_at BIGINT NOT NULL,
 UNIQUE(match_id,version)
);
CREATE FUNCTION procurement_bill_content_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' OR (to_jsonb(NEW)-ARRAY['status','version','reviewed_by','review_reason','variance_account_id','invoice_id','updated_at']) IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['status','version','reviewed_by','review_reason','variance_account_id','invoice_id','updated_at']) THEN
 RAISE EXCEPTION 'SUPPLIER_BILL_EVIDENCE_IMMUTABLE'; END IF; RETURN NEW;
END $$;
CREATE TRIGGER supplier_bill_content BEFORE UPDATE OR DELETE ON procurement_bill_matches FOR EACH ROW EXECUTE FUNCTION procurement_bill_content_guard();
CREATE FUNCTION procurement_bill_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'SUPPLIER_BILL_EVIDENCE_IMMUTABLE'; END $$;
DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['procurement_bill_match_lines','procurement_bill_match_events'] LOOP
 EXECUTE format('CREATE TRIGGER supplier_bill_immutable BEFORE UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION procurement_bill_immutable()',t);
 END LOOP;
 FOREACH t IN ARRAY ARRAY['procurement_bill_matches','procurement_bill_match_lines','procurement_bill_match_events'] LOOP
 INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
 EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
 EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
 END LOOP;
END $$;
