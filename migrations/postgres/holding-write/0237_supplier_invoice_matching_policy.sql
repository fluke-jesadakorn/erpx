CREATE TABLE procurement_invoice_policies (
 company_id TEXT PRIMARY KEY,
 matching_mode TEXT NOT NULL CHECK(matching_mode IN ('optional','required')),
 version BIGINT NOT NULL CHECK(version>0),
 updated_by TEXT NOT NULL,
 updated_at BIGINT NOT NULL
);

CREATE TABLE procurement_invoice_policy_events (
 id TEXT PRIMARY KEY,
 company_id TEXT NOT NULL REFERENCES procurement_invoice_policies(company_id),
 version BIGINT NOT NULL CHECK(version>0),
 previous_mode TEXT NOT NULL CHECK(previous_mode IN ('optional','required')),
 matching_mode TEXT NOT NULL CHECK(matching_mode IN ('optional','required')),
 actor_id TEXT NOT NULL,
 reason TEXT NOT NULL,
 created_at BIGINT NOT NULL,
 UNIQUE(company_id,version)
);

CREATE FUNCTION procurement_invoice_policy_event_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 RAISE EXCEPTION 'PROCUREMENT_POLICY_HISTORY_IMMUTABLE';
END $$;
CREATE TRIGGER procurement_invoice_policy_event_content
 BEFORE UPDATE OR DELETE ON procurement_invoice_policy_events
 FOR EACH ROW EXECUTE FUNCTION procurement_invoice_policy_event_guard();

INSERT INTO work_cache_revisions(table_name) VALUES('procurement_invoice_policies') ON CONFLICT DO NOTHING;
CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON procurement_invoice_policies
 FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON procurement_invoice_policies
 FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed();
