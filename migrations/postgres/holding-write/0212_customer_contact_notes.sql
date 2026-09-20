-- Internal contact evidence stays with its scoped commercial source, never a portal projection.
CREATE TABLE customer_contact_notes (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, customer_id TEXT NOT NULL REFERENCES finance_parties(id),
 order_id TEXT NOT NULL REFERENCES commerce_orders(id), channel TEXT NOT NULL CHECK(channel IN ('phone','email','meeting','other')),
 occurred_at BIGINT NOT NULL, body TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','archived')),
 version BIGINT NOT NULL DEFAULT 1, created_by TEXT NOT NULL, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
);
CREATE INDEX customer_contact_notes_source ON customer_contact_notes(company_id,customer_id,order_id,created_at);
CREATE TABLE customer_contact_note_history (
 id TEXT PRIMARY KEY, note_id TEXT NOT NULL REFERENCES customer_contact_notes(id), version BIGINT NOT NULL,
 action TEXT NOT NULL CHECK(action IN ('create','correct','archive')), body TEXT NOT NULL, reason TEXT NOT NULL,
 actor_id TEXT NOT NULL, created_at BIGINT NOT NULL, UNIQUE(note_id,version)
);
CREATE FUNCTION customer_contact_evidence_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_TABLE_NAME='customer_contact_note_history' OR TG_OP='DELETE' THEN RAISE EXCEPTION 'CUSTOMER_NOTE_EVIDENCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF (NEW.id,NEW.company_id,NEW.customer_id,NEW.order_id,NEW.channel,NEW.occurred_at,NEW.created_by,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.customer_id,OLD.order_id,OLD.channel,OLD.occurred_at,OLD.created_by,OLD.created_at) THEN RAISE EXCEPTION 'CUSTOMER_NOTE_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER customer_note_guard BEFORE UPDATE OR DELETE ON customer_contact_notes FOR EACH ROW EXECUTE FUNCTION customer_contact_evidence_guard();
CREATE TRIGGER customer_note_history_guard BEFORE UPDATE OR DELETE ON customer_contact_note_history FOR EACH ROW EXECUTE FUNCTION customer_contact_evidence_guard();
-- Do not move existing contact evidence to a different customer or authorization scope.
CREATE FUNCTION customer_contact_source_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (NEW.party_id,NEW.company_id,NEW.branch_id,NEW.department_id) IS DISTINCT FROM (OLD.party_id,OLD.company_id,OLD.branch_id,OLD.department_id)
 AND EXISTS(SELECT 1 FROM customer_contact_notes WHERE order_id=OLD.id) THEN
  RAISE EXCEPTION 'CUSTOMER_CONTACT_SOURCE_HAS_EVIDENCE' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER customer_contact_order_guard BEFORE UPDATE ON commerce_orders FOR EACH ROW EXECUTE FUNCTION customer_contact_source_guard();
