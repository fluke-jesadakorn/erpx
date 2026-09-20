CREATE TABLE customer_identity_changes (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,customer_id TEXT NOT NULL REFERENCES finance_parties(id),
 expected_customer_version BIGINT NOT NULL,previous_json JSONB NOT NULL,proposed_json JSONB NOT NULL,
 status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','rejected','applied')),evidence TEXT NOT NULL,
 created_by TEXT NOT NULL,reviewed_by TEXT,version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL
);
CREATE INDEX customer_identity_change_queue ON customer_identity_changes(company_id,status,updated_at);
CREATE TABLE customer_identity_change_events (
 id TEXT PRIMARY KEY,change_id TEXT NOT NULL REFERENCES customer_identity_changes(id),version BIGINT NOT NULL,
 action TEXT NOT NULL CHECK(action IN ('request','reject','resubmit','apply')),actor_id TEXT NOT NULL,reason TEXT NOT NULL,
 customer_version BIGINT NOT NULL,previous_json JSONB NOT NULL,proposed_json JSONB NOT NULL,created_at BIGINT NOT NULL,UNIQUE(change_id,version)
);
CREATE FUNCTION customer_identity_change_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_TABLE_NAME='customer_identity_change_events' OR TG_OP='DELETE' THEN RAISE EXCEPTION 'CUSTOMER_IDENTITY_HISTORY_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF (NEW.id,NEW.company_id,NEW.customer_id,NEW.created_by,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.customer_id,OLD.created_by,OLD.created_at) THEN RAISE EXCEPTION 'CUSTOMER_IDENTITY_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER customer_identity_change_record_guard BEFORE UPDATE OR DELETE ON customer_identity_changes FOR EACH ROW EXECUTE FUNCTION customer_identity_change_guard();
CREATE TRIGGER customer_identity_change_event_guard BEFORE UPDATE OR DELETE ON customer_identity_change_events FOR EACH ROW EXECUTE FUNCTION customer_identity_change_guard();
