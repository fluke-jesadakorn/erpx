CREATE TABLE customer_followups (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, order_id TEXT NOT NULL REFERENCES commerce_orders(id),
 owner_id TEXT NOT NULL, due_date TEXT NOT NULL, next_action TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','completed','cancelled')),
 outcome TEXT, version BIGINT NOT NULL DEFAULT 1, created_by TEXT NOT NULL, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
);
CREATE TABLE customer_followup_events (
 id TEXT PRIMARY KEY, followup_id TEXT NOT NULL REFERENCES customer_followups(id), version BIGINT NOT NULL,
 action TEXT NOT NULL CHECK(action IN ('create','complete','cancel','reassign')), actor_id TEXT NOT NULL,
 owner_id TEXT NOT NULL, due_date TEXT NOT NULL, reason TEXT NOT NULL, created_at BIGINT NOT NULL, UNIQUE(followup_id,version)
);
CREATE INDEX customer_followups_owner ON customer_followups(company_id,owner_id,status,due_date);
CREATE FUNCTION customer_followup_history_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'CUSTOMER_FOLLOWUP_HISTORY_IMMUTABLE' USING ERRCODE='23514'; END $$;
CREATE TRIGGER customer_followup_history BEFORE UPDATE OR DELETE ON customer_followup_events FOR EACH ROW EXECUTE FUNCTION customer_followup_history_guard();
CREATE OR REPLACE FUNCTION customer_contact_source_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (NEW.party_id,NEW.company_id,NEW.branch_id,NEW.department_id) IS DISTINCT FROM (OLD.party_id,OLD.company_id,OLD.branch_id,OLD.department_id)
 AND (EXISTS(SELECT 1 FROM customer_contact_notes WHERE order_id=OLD.id) OR EXISTS(SELECT 1 FROM customer_followups WHERE order_id=OLD.id)) THEN
  RAISE EXCEPTION 'CUSTOMER_CONTACT_SOURCE_HAS_EVIDENCE' USING ERRCODE='23514';
 END IF; RETURN NEW;
END $$;
DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['customer_followups','customer_followup_events'] LOOP
 INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
 EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
 EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
 END LOOP;
END $$;
