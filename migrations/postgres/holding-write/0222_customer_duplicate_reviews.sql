CREATE TABLE customer_duplicate_reviews (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,left_customer_id TEXT NOT NULL REFERENCES finance_parties(id),
 right_customer_id TEXT NOT NULL REFERENCES finance_parties(id),status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','distinct','duplicate')),
 canonical_customer_id TEXT REFERENCES finance_parties(id),evidence TEXT NOT NULL,created_by TEXT NOT NULL,reviewed_by TEXT,
 version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 CHECK(left_customer_id<right_customer_id),CHECK(canonical_customer_id IS NULL OR canonical_customer_id IN(left_customer_id,right_customer_id)),
 CHECK((status='duplicate')=(canonical_customer_id IS NOT NULL)),UNIQUE(company_id,left_customer_id,right_customer_id)
);
CREATE TABLE customer_duplicate_events (
 id TEXT PRIMARY KEY,review_id TEXT NOT NULL REFERENCES customer_duplicate_reviews(id),version BIGINT NOT NULL,
 action TEXT NOT NULL CHECK(action IN ('request','distinct','duplicate','reopen')),actor_id TEXT NOT NULL,
 reason TEXT NOT NULL,canonical_customer_id TEXT,created_at BIGINT NOT NULL,UNIQUE(review_id,version)
);
CREATE FUNCTION customer_duplicate_evidence_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_TABLE_NAME='customer_duplicate_events' OR TG_OP='DELETE' THEN RAISE EXCEPTION 'CUSTOMER_DUPLICATE_HISTORY_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF (NEW.id,NEW.company_id,NEW.left_customer_id,NEW.right_customer_id,NEW.created_by,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.left_customer_id,OLD.right_customer_id,OLD.created_by,OLD.created_at) THEN RAISE EXCEPTION 'CUSTOMER_DUPLICATE_PAIR_IMMUTABLE' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER customer_duplicate_review_guard BEFORE UPDATE OR DELETE ON customer_duplicate_reviews FOR EACH ROW EXECUTE FUNCTION customer_duplicate_evidence_guard();
CREATE TRIGGER customer_duplicate_event_guard BEFORE UPDATE OR DELETE ON customer_duplicate_events FOR EACH ROW EXECUTE FUNCTION customer_duplicate_evidence_guard();
