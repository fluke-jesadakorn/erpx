CREATE TABLE procurement_bill_match_receipts (
 id TEXT PRIMARY KEY,
 match_id TEXT NOT NULL REFERENCES procurement_bill_matches(id),
 fulfillment_id TEXT NOT NULL REFERENCES commerce_fulfillments(id),
 captured_version BIGINT NOT NULL CHECK(captured_version>0),
 active BOOLEAN NOT NULL DEFAULT TRUE,
 UNIQUE(match_id,fulfillment_id)
);

INSERT INTO procurement_bill_match_receipts(id,match_id,fulfillment_id,captured_version,active)
SELECT m.id||'-receipt-0',m.id,m.fulfillment_id,f.version,m.status<>'returned'
FROM procurement_bill_matches m JOIN commerce_fulfillments f ON f.id=m.fulfillment_id;

ALTER TABLE commerce_fulfillments DROP CONSTRAINT commerce_fulfillments_invoice_id_key;
CREATE INDEX commerce_fulfillments_invoice_id ON commerce_fulfillments(invoice_id) WHERE invoice_id IS NOT NULL;

DROP INDEX procurement_bill_active_receipt;
CREATE UNIQUE INDEX procurement_bill_active_receipt
 ON procurement_bill_match_receipts(fulfillment_id) WHERE active;

CREATE FUNCTION procurement_bill_receipt_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE'
    OR (to_jsonb(NEW)-'active') IS DISTINCT FROM (to_jsonb(OLD)-'active')
    OR OLD.active=FALSE OR NEW.active=TRUE THEN
  RAISE EXCEPTION 'SUPPLIER_BILL_EVIDENCE_IMMUTABLE';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER supplier_bill_receipt_content
 BEFORE UPDATE OR DELETE ON procurement_bill_match_receipts
 FOR EACH ROW EXECUTE FUNCTION procurement_bill_receipt_guard();

INSERT INTO work_cache_revisions(table_name) VALUES('procurement_bill_match_receipts') ON CONFLICT DO NOTHING;
CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON procurement_bill_match_receipts
 FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON procurement_bill_match_receipts
 FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed();
