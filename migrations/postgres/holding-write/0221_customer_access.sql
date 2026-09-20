-- Explicit, source-bound customer access. Never inferred from following or mentions.
CREATE TABLE customer_access_bindings (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, order_id TEXT NOT NULL REFERENCES commerce_orders(id),
 customer_id TEXT NOT NULL REFERENCES finance_parties(id), email TEXT NOT NULL,
 expires_at BIGINT NOT NULL, status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','approved','rejected','revoked')),
 reason TEXT NOT NULL, created_by TEXT NOT NULL, approved_by TEXT, reviewed_at BIGINT,
 version BIGINT NOT NULL DEFAULT 1, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
 CHECK(email=lower(email)), CHECK(expires_at>created_at), CHECK(approved_by IS NULL OR approved_by<>created_by)
);
CREATE INDEX customer_access_lookup ON customer_access_bindings(email,status,expires_at);
CREATE TABLE customer_access_events (
 id TEXT PRIMARY KEY, binding_id TEXT NOT NULL REFERENCES customer_access_bindings(id), version BIGINT NOT NULL,
 action TEXT NOT NULL, actor_id TEXT NOT NULL, reason TEXT NOT NULL, created_at BIGINT NOT NULL, UNIQUE(binding_id,version)
);
CREATE TABLE customer_access_reads (
 id TEXT PRIMARY KEY, binding_id TEXT NOT NULL REFERENCES customer_access_bindings(id), binding_version BIGINT NOT NULL,
 user_id TEXT NOT NULL, read_at BIGINT NOT NULL, projection_version TEXT NOT NULL
);
CREATE TABLE customer_access_identities (
 binding_id TEXT PRIMARY KEY REFERENCES customer_access_bindings(id), user_id TEXT NOT NULL,
 email TEXT NOT NULL, bound_at BIGINT NOT NULL, CHECK(email=lower(email))
);
CREATE FUNCTION customer_access_evidence_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_TABLE_NAME<>'customer_access_bindings' OR TG_OP='DELETE' THEN RAISE EXCEPTION 'CUSTOMER_ACCESS_HISTORY_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF (NEW.id,NEW.company_id,NEW.order_id,NEW.customer_id,NEW.email,NEW.expires_at,NEW.created_by,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.order_id,OLD.customer_id,OLD.email,OLD.expires_at,OLD.created_by,OLD.created_at) THEN RAISE EXCEPTION 'CUSTOMER_ACCESS_SCOPE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER customer_access_binding_guard BEFORE UPDATE OR DELETE ON customer_access_bindings FOR EACH ROW EXECUTE FUNCTION customer_access_evidence_guard();
CREATE TRIGGER customer_access_event_guard BEFORE UPDATE OR DELETE ON customer_access_events FOR EACH ROW EXECUTE FUNCTION customer_access_evidence_guard();
CREATE TRIGGER customer_access_read_guard BEFORE UPDATE OR DELETE ON customer_access_reads FOR EACH ROW EXECUTE FUNCTION customer_access_evidence_guard();
CREATE TRIGGER customer_access_identity_guard BEFORE UPDATE OR DELETE ON customer_access_identities FOR EACH ROW EXECUTE FUNCTION customer_access_evidence_guard();
CREATE FUNCTION customer_access_source_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (NEW.party_id,NEW.company_id,NEW.branch_id,NEW.department_id) IS DISTINCT FROM (OLD.party_id,OLD.company_id,OLD.branch_id,OLD.department_id)
 AND EXISTS(SELECT 1 FROM customer_access_bindings WHERE order_id=OLD.id) THEN RAISE EXCEPTION 'CUSTOMER_ACCESS_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER customer_access_order_guard BEFORE UPDATE ON commerce_orders FOR EACH ROW EXECUTE FUNCTION customer_access_source_guard();
