-- External customer actions are source-bound, idempotent and retained as immutable evidence.
CREATE TABLE customer_portal_actions (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  binding_id TEXT NOT NULL REFERENCES customer_access_bindings(id),
  order_id TEXT NOT NULL REFERENCES commerce_orders(id),
  customer_id TEXT NOT NULL REFERENCES finance_parties(id),
  user_id TEXT NOT NULL,
  action TEXT NOT NULL CHECK(action IN ('acknowledge_delivery','request_support')),
  note TEXT NOT NULL DEFAULT '',
  source_version BIGINT NOT NULL,
  request_key TEXT NOT NULL UNIQUE,
  request_hash TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'recorded' CHECK(status='recorded'),
  created_at BIGINT NOT NULL,
  UNIQUE(binding_id,action)
);
CREATE INDEX customer_portal_action_order ON customer_portal_actions(company_id,order_id,created_at);
CREATE FUNCTION customer_portal_action_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP='DELETE' OR TG_OP='UPDATE' THEN
    RAISE EXCEPTION 'CUSTOMER_PORTAL_ACTION_IMMUTABLE' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER customer_portal_action_guard BEFORE UPDATE OR DELETE ON customer_portal_actions FOR EACH ROW EXECUTE FUNCTION customer_portal_action_guard();
