-- Private proof-of-delivery files are captured before a sales stock movement and linked immutably to that delivery.
ALTER TABLE commerce_fulfillments ADD COLUMN evidence_note TEXT;

CREATE TABLE commerce_fulfillment_evidence (
 fulfillment_id TEXT NOT NULL REFERENCES commerce_fulfillments(id),
 attachment_id TEXT NOT NULL UNIQUE REFERENCES attachments(id),
 linked_by TEXT NOT NULL,
 linked_at BIGINT NOT NULL,
 PRIMARY KEY(fulfillment_id,attachment_id)
);

CREATE FUNCTION commerce_fulfillment_evidence_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 RAISE EXCEPTION 'FULFILLMENT_EVIDENCE_IMMUTABLE' USING ERRCODE='23514';
END $$;
CREATE TRIGGER commerce_fulfillment_evidence_immutable BEFORE UPDATE OR DELETE ON commerce_fulfillment_evidence FOR EACH ROW EXECUTE FUNCTION commerce_fulfillment_evidence_immutable();

INSERT INTO work_cache_revisions(table_name) VALUES('commerce_fulfillment_evidence') ON CONFLICT DO NOTHING;
CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON commerce_fulfillment_evidence FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON commerce_fulfillment_evidence FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed();
