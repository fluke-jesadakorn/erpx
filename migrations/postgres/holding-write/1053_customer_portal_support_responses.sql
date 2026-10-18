-- Portal-owned response delivery is visibility inside the verified document,
-- never evidence that an email or another external provider sent a message.
-- Original immutable customer_portal_actions and all retained files stay intact.
CREATE TABLE customer_portal_support_responses (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL REFERENCES organization_nodes(id),
  request_id TEXT NOT NULL REFERENCES customer_portal_actions(id),
  binding_id TEXT NOT NULL REFERENCES customer_access_bindings(id),
  order_id TEXT NOT NULL REFERENCES commerce_orders(id),
  customer_id TEXT NOT NULL REFERENCES finance_parties(id),
  customer_user_id TEXT NOT NULL,
  source_version BIGINT NOT NULL CHECK(source_version>0),
  binding_version BIGINT NOT NULL CHECK(binding_version>0),
  response_version BIGINT NOT NULL CHECK(response_version>0),
  message TEXT NOT NULL CHECK(length(message) BETWEEN 3 AND 4000),
  reason TEXT NOT NULL CHECK(length(reason) BETWEEN 3 AND 2000),
  actor_id TEXT NOT NULL,
  command_id TEXT NOT NULL UNIQUE REFERENCES command_receipts(command_id) DEFERRABLE INITIALLY DEFERRED,
  created_at BIGINT NOT NULL,
  UNIQUE(request_id,response_version)
);
CREATE INDEX customer_portal_response_company_request ON customer_portal_support_responses(company_id,request_id,response_version DESC);
CREATE TABLE customer_portal_support_acknowledgements (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL REFERENCES organization_nodes(id),
  request_id TEXT NOT NULL UNIQUE REFERENCES customer_portal_actions(id),
  response_id TEXT NOT NULL UNIQUE REFERENCES customer_portal_support_responses(id),
  response_version BIGINT NOT NULL CHECK(response_version>0),
  binding_id TEXT NOT NULL REFERENCES customer_access_bindings(id),
  order_id TEXT NOT NULL REFERENCES commerce_orders(id),
  customer_id TEXT NOT NULL REFERENCES finance_parties(id),
  user_id TEXT NOT NULL,
  binding_version BIGINT NOT NULL CHECK(binding_version>0),
  request_key TEXT NOT NULL UNIQUE,
  request_hash TEXT NOT NULL,
  note TEXT NOT NULL DEFAULT '' CHECK(length(note)<=500),
  created_at BIGINT NOT NULL
);
CREATE INDEX customer_portal_ack_company_binding ON customer_portal_support_acknowledgements(company_id,binding_id);
CREATE FUNCTION customer_portal_support_response_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP<>'INSERT' THEN RAISE EXCEPTION 'CUSTOMER_PORTAL_RESPONSE_IMMUTABLE' USING ERRCODE='23514'; END IF;
  IF NOT EXISTS(SELECT 1 FROM customer_portal_actions request JOIN customer_access_bindings binding ON binding.id=request.binding_id JOIN customer_access_identities identity ON identity.binding_id=binding.id AND identity.user_id=request.user_id AND identity.email=binding.email
    WHERE request.id=NEW.request_id AND request.action='request_support' AND request.company_id=NEW.company_id AND request.binding_id=NEW.binding_id AND request.order_id=NEW.order_id AND request.customer_id=NEW.customer_id AND request.user_id=NEW.customer_user_id AND request.source_version=NEW.source_version AND binding.company_id=NEW.company_id AND binding.order_id=NEW.order_id AND binding.customer_id=NEW.customer_id)
    OR EXISTS(SELECT 1 FROM customer_portal_support_acknowledgements acknowledgement WHERE acknowledgement.request_id=NEW.request_id)
    OR EXISTS(SELECT 1 FROM customer_portal_support_closures closure WHERE closure.request_id=NEW.request_id)
    OR NEW.response_version<>COALESCE((SELECT max(response.response_version) FROM customer_portal_support_responses response WHERE response.request_id=NEW.request_id),0)+1
  THEN RAISE EXCEPTION 'CUSTOMER_PORTAL_RESPONSE_SOURCE_CHANGED' USING ERRCODE='23514'; END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER customer_portal_response_guard BEFORE INSERT OR UPDATE OR DELETE ON customer_portal_support_responses FOR EACH ROW EXECUTE FUNCTION customer_portal_support_response_guard();
CREATE FUNCTION customer_portal_support_ack_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP<>'INSERT' THEN RAISE EXCEPTION 'CUSTOMER_PORTAL_ACK_IMMUTABLE' USING ERRCODE='23514'; END IF;
  IF NOT EXISTS(SELECT 1 FROM customer_portal_support_responses response JOIN customer_portal_actions request ON request.id=response.request_id JOIN customer_access_identities identity ON identity.binding_id=response.binding_id AND identity.user_id=NEW.user_id
    WHERE response.id=NEW.response_id AND response.response_version=NEW.response_version AND response.request_id=NEW.request_id AND response.company_id=NEW.company_id AND response.binding_id=NEW.binding_id AND response.order_id=NEW.order_id AND response.customer_id=NEW.customer_id AND response.customer_user_id=NEW.user_id AND request.action='request_support' AND request.user_id=NEW.user_id AND response.response_version=(SELECT max(latest.response_version) FROM customer_portal_support_responses latest WHERE latest.request_id=NEW.request_id))
  OR EXISTS(SELECT 1 FROM customer_portal_support_closures closure WHERE closure.request_id=NEW.request_id)
  THEN RAISE EXCEPTION 'CUSTOMER_PORTAL_ACK_SOURCE_CHANGED' USING ERRCODE='23514'; END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER customer_portal_ack_guard BEFORE INSERT OR UPDATE OR DELETE ON customer_portal_support_acknowledgements FOR EACH ROW EXECUTE FUNCTION customer_portal_support_ack_guard();
-- Independent closure is explicitly different from customer acknowledgement.
-- It is available only after the approved customer access is no longer valid.
CREATE TABLE customer_portal_support_closures (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL REFERENCES organization_nodes(id),
  request_id TEXT NOT NULL UNIQUE REFERENCES customer_portal_actions(id),
  binding_id TEXT NOT NULL REFERENCES customer_access_bindings(id),
  binding_version BIGINT NOT NULL CHECK(binding_version>0),
  response_version BIGINT NOT NULL CHECK(response_version>=0),
  outcome TEXT NOT NULL CHECK(outcome='closed_without_customer_acknowledgement'),
  reason TEXT NOT NULL CHECK(length(reason) BETWEEN 3 AND 2000),
  actor_id TEXT NOT NULL,
  command_id TEXT NOT NULL UNIQUE REFERENCES command_receipts(command_id) DEFERRABLE INITIALLY DEFERRED,
  created_at BIGINT NOT NULL
);
CREATE INDEX customer_portal_support_closure_company ON customer_portal_support_closures(company_id,binding_id);
CREATE FUNCTION customer_portal_support_closure_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP<>'INSERT' THEN RAISE EXCEPTION 'CUSTOMER_PORTAL_CLOSURE_IMMUTABLE' USING ERRCODE='23514'; END IF;
  IF NOT EXISTS(SELECT 1 FROM customer_portal_actions request JOIN customer_access_bindings binding ON binding.id=request.binding_id
    WHERE request.id=NEW.request_id AND request.action='request_support' AND request.company_id=NEW.company_id AND request.binding_id=NEW.binding_id AND binding.company_id=NEW.company_id AND binding.version=NEW.binding_version AND (binding.status IN ('rejected','revoked') OR binding.expires_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND binding.created_by<>NEW.actor_id)
    OR NEW.response_version<>COALESCE((SELECT max(response.response_version) FROM customer_portal_support_responses response WHERE response.request_id=NEW.request_id),0)
    OR EXISTS(SELECT 1 FROM customer_portal_support_responses response WHERE response.request_id=NEW.request_id AND response.response_version=NEW.response_version AND response.actor_id=NEW.actor_id)
    OR EXISTS(SELECT 1 FROM customer_portal_support_acknowledgements acknowledgement WHERE acknowledgement.request_id=NEW.request_id)
  THEN RAISE EXCEPTION 'CUSTOMER_PORTAL_CLOSURE_SOURCE_CHANGED' USING ERRCODE='23514'; END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER customer_portal_closure_guard BEFORE INSERT OR UPDATE OR DELETE ON customer_portal_support_closures FOR EACH ROW EXECUTE FUNCTION customer_portal_support_closure_guard();
