-- A Support-owned ticket invitation is independent of Sales document access.
-- The reviewed customer summary never republishes the private issue/events.
CREATE TABLE support_ticket_portal_bindings(
 id text PRIMARY KEY,company_id text NOT NULL REFERENCES organization_nodes(id),
 ticket_id text NOT NULL REFERENCES support_tickets(id),ticket_version bigint NOT NULL CHECK(ticket_version>0),
 source_identity jsonb NOT NULL,customer_name text NOT NULL CHECK(length(customer_name) BETWEEN 1 AND 160),
 email text NOT NULL CHECK(email=lower(email) AND length(email)>3),subject text NOT NULL CHECK(length(subject) BETWEEN 3 AND 160),
 summary text NOT NULL CHECK(length(summary) BETWEEN 3 AND 4000),expires_at bigint NOT NULL,
 status text NOT NULL CHECK(status IN('draft','approved','rejected','revoked')),version bigint NOT NULL CHECK(version>0),
 prepared_by text NOT NULL,prepare_command_id text NOT NULL UNIQUE REFERENCES command_receipts(command_id) DEFERRABLE INITIALLY DEFERRED,
 created_at bigint NOT NULL,updated_at bigint NOT NULL
);
CREATE INDEX support_ticket_portal_email ON support_ticket_portal_bindings(email,status,expires_at);
CREATE INDEX support_ticket_portal_company_ticket ON support_ticket_portal_bindings(company_id,ticket_id);
CREATE TABLE support_ticket_portal_events(
 id text PRIMARY KEY,binding_id text NOT NULL REFERENCES support_ticket_portal_bindings(id),company_id text NOT NULL,
 version bigint NOT NULL CHECK(version>0),action text NOT NULL CHECK(action IN('prepare','approve','reject','revoke')),
 actor_id text NOT NULL,command_id text NOT NULL UNIQUE REFERENCES command_receipts(command_id) DEFERRABLE INITIALLY DEFERRED,
 reason text NOT NULL CHECK(length(reason) BETWEEN 3 AND 2000),created_at bigint NOT NULL,UNIQUE(binding_id,version)
);
CREATE TABLE support_ticket_portal_identities(
 binding_id text PRIMARY KEY REFERENCES support_ticket_portal_bindings(id),company_id text NOT NULL,
 user_id text NOT NULL,email text NOT NULL,created_at bigint NOT NULL
);
CREATE TABLE support_ticket_portal_reads(
 id text PRIMARY KEY,binding_id text NOT NULL REFERENCES support_ticket_portal_bindings(id),company_id text NOT NULL,
 user_id text NOT NULL,binding_version bigint NOT NULL CHECK(binding_version>0),created_at bigint NOT NULL
);
CREATE TABLE support_ticket_portal_responses(
 id text PRIMARY KEY,company_id text NOT NULL,binding_id text NOT NULL REFERENCES support_ticket_portal_bindings(id),
 binding_version bigint NOT NULL CHECK(binding_version>0),response_version bigint NOT NULL CHECK(response_version>0),
 message text NOT NULL CHECK(length(message) BETWEEN 3 AND 4000),reason text NOT NULL CHECK(length(reason) BETWEEN 3 AND 2000),
 actor_id text NOT NULL,command_id text NOT NULL UNIQUE REFERENCES command_receipts(command_id) DEFERRABLE INITIALLY DEFERRED,
 created_at bigint NOT NULL,UNIQUE(binding_id,response_version)
);
CREATE TABLE support_ticket_portal_acknowledgements(
 id text PRIMARY KEY,company_id text NOT NULL,binding_id text NOT NULL REFERENCES support_ticket_portal_bindings(id),
 binding_version bigint NOT NULL CHECK(binding_version>0),response_id text NOT NULL UNIQUE REFERENCES support_ticket_portal_responses(id),
 response_version bigint NOT NULL CHECK(response_version>0),user_id text NOT NULL,email text NOT NULL,
 request_key text NOT NULL UNIQUE,request_hash text NOT NULL,note text NOT NULL CHECK(length(note)<=500),created_at bigint NOT NULL
);
CREATE INDEX support_ticket_portal_reads_binding ON support_ticket_portal_reads(binding_id);
CREATE INDEX support_ticket_portal_acks_binding ON support_ticket_portal_acknowledgements(binding_id);
CREATE FUNCTION support_ticket_portal_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP<>'INSERT' THEN RAISE EXCEPTION 'SUPPORT_PORTAL_SOURCE_CHANGED' USING ERRCODE='23514';END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER support_ticket_portal_event_immutable BEFORE UPDATE OR DELETE ON support_ticket_portal_events FOR EACH ROW EXECUTE FUNCTION support_ticket_portal_immutable();
CREATE TRIGGER support_ticket_portal_identity_immutable BEFORE UPDATE OR DELETE ON support_ticket_portal_identities FOR EACH ROW EXECUTE FUNCTION support_ticket_portal_immutable();
CREATE TRIGGER support_ticket_portal_read_immutable BEFORE UPDATE OR DELETE ON support_ticket_portal_reads FOR EACH ROW EXECUTE FUNCTION support_ticket_portal_immutable();
CREATE TRIGGER support_ticket_portal_response_immutable BEFORE UPDATE OR DELETE ON support_ticket_portal_responses FOR EACH ROW EXECUTE FUNCTION support_ticket_portal_immutable();
CREATE TRIGGER support_ticket_portal_ack_immutable BEFORE UPDATE OR DELETE ON support_ticket_portal_acknowledgements FOR EACH ROW EXECUTE FUNCTION support_ticket_portal_immutable();
CREATE FUNCTION support_ticket_portal_binding_birth() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' THEN RAISE EXCEPTION 'SUPPORT_PORTAL_SOURCE_CHANGED' USING ERRCODE='23514';END IF;
 IF TG_OP='INSERT' THEN
  IF NEW.status<>'draft' OR NEW.version<>1 OR NEW.id<>'support-portal-'||NEW.prepare_command_id THEN RAISE EXCEPTION 'SUPPORT_PORTAL_SOURCE_CHANGED' USING ERRCODE='23514';END IF;
 ELSIF (to_jsonb(OLD)-ARRAY['status','version','updated_at']) IS DISTINCT FROM (to_jsonb(NEW)-ARRAY['status','version','updated_at'])
  OR NEW.version<>OLD.version+1 OR NOT(OLD.status='draft' AND NEW.status IN('approved','rejected') OR OLD.status='approved' AND NEW.status='revoked') THEN
  RAISE EXCEPTION 'SUPPORT_PORTAL_SOURCE_CHANGED' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER support_ticket_portal_binding_birth BEFORE INSERT OR UPDATE OR DELETE ON support_ticket_portal_bindings FOR EACH ROW EXECUTE FUNCTION support_ticket_portal_binding_birth();
CREATE FUNCTION support_ticket_portal_assert(binding_key text) RETURNS void LANGUAGE plpgsql AS $$
DECLARE binding support_ticket_portal_bindings%ROWTYPE;
BEGIN
 SELECT * INTO binding FROM support_ticket_portal_bindings WHERE id=binding_key;
 IF NOT FOUND THEN RAISE EXCEPTION 'SUPPORT_PORTAL_SOURCE_CHANGED' USING ERRCODE='23514';END IF;
 IF NOT EXISTS(SELECT 1 FROM support_tickets ticket WHERE ticket.id=binding.ticket_id AND ticket.company_id=binding.company_id AND ticket.source_kind<>'field_service' AND binding.source_identity=jsonb_build_object('companyId',ticket.company_id,'branchId',ticket.branch_id,'departmentId',ticket.department_id,'sourceKind',ticket.source_kind,'orderId',ticket.order_id,'customerId',ticket.customer_id,'sourceRecord',ticket.source_record_json))
 OR NOT EXISTS(SELECT 1 FROM command_receipts receipt JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.action=receipt.operation AND audit.record_id=binding.id AND audit.record_version=1 AND audit.actor_id=binding.prepared_by JOIN outbox_events event ON event.command_id=receipt.command_id AND event.event_type=receipt.operation AND event.aggregate_id=binding.id AND event.aggregate_version=1 WHERE receipt.command_id=binding.prepare_command_id AND receipt.operation='support.ticket-portal.prepare' AND receipt.actor_id=binding.prepared_by AND receipt.record_id=binding.id AND receipt.authority_json::jsonb->>'companyId'=binding.company_id AND receipt.result_json::jsonb=jsonb_build_object('id',binding.id,'status','draft','version',1))
 OR NOT EXISTS(SELECT 1 FROM support_ticket_portal_events current JOIN command_receipts receipt ON receipt.command_id=current.command_id AND receipt.record_id=binding.id AND receipt.actor_id=current.actor_id AND receipt.operation='support.ticket-portal.'||current.action AND receipt.authority_json::jsonb->>'companyId'=binding.company_id JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.action=receipt.operation AND audit.record_id=binding.id AND audit.record_version=binding.version AND audit.actor_id=current.actor_id JOIN outbox_events outbox ON outbox.command_id=receipt.command_id AND outbox.event_type=receipt.operation AND outbox.aggregate_id=binding.id AND outbox.aggregate_version=binding.version WHERE current.binding_id=binding.id AND current.company_id=binding.company_id AND current.version=binding.version AND current.action=CASE binding.status WHEN 'draft' THEN 'prepare' WHEN 'approved' THEN 'approve' WHEN 'rejected' THEN 'reject' ELSE 'revoke' END AND receipt.result_json::jsonb=jsonb_build_object('id',binding.id,'status',binding.status,'version',binding.version) AND (current.action NOT IN('approve','reject') OR current.actor_id<>binding.prepared_by))
 OR EXISTS(SELECT 1 FROM support_ticket_portal_events history WHERE history.binding_id=binding.id AND (history.company_id<>binding.company_id OR history.version>binding.version OR history.action IN('approve','reject') AND history.actor_id=binding.prepared_by OR NOT EXISTS(SELECT 1 FROM command_receipts receipt JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.action=receipt.operation AND audit.record_id=binding.id AND audit.record_version=history.version AND audit.actor_id=history.actor_id JOIN outbox_events outbox ON outbox.command_id=receipt.command_id AND outbox.event_type=receipt.operation AND outbox.aggregate_id=binding.id AND outbox.aggregate_version=history.version WHERE receipt.command_id=history.command_id AND receipt.operation='support.ticket-portal.'||history.action AND receipt.record_id=binding.id AND receipt.actor_id=history.actor_id AND receipt.authority_json::jsonb->>'companyId'=binding.company_id AND receipt.result_json::jsonb=jsonb_build_object('id',binding.id,'status',CASE history.action WHEN 'prepare' THEN 'draft' WHEN 'approve' THEN 'approved' WHEN 'reject' THEN 'rejected' ELSE 'revoked' END,'version',history.version))))
 OR (SELECT count(*) FROM support_ticket_portal_events event WHERE event.binding_id=binding.id)<>binding.version
 THEN RAISE EXCEPTION 'SUPPORT_PORTAL_SOURCE_CHANGED' USING ERRCODE='23514';END IF;
 IF EXISTS(SELECT 1 FROM support_ticket_portal_identities identity WHERE identity.binding_id=binding.id AND (identity.company_id<>binding.company_id OR identity.email<>binding.email))
 OR EXISTS(SELECT 1 FROM support_ticket_portal_reads read LEFT JOIN support_ticket_portal_identities identity ON identity.binding_id=read.binding_id AND identity.user_id=read.user_id WHERE read.binding_id=binding.id AND (read.company_id<>binding.company_id OR read.binding_version>binding.version OR identity.binding_id IS NULL))
 OR EXISTS(SELECT 1 FROM support_ticket_portal_responses response WHERE response.binding_id=binding.id AND (response.company_id<>binding.company_id OR response.binding_version>binding.version OR response.actor_id=binding.prepared_by OR NOT EXISTS(SELECT 1 FROM command_receipts receipt JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=binding.id AND audit.record_version=response.response_version AND audit.action=receipt.operation AND audit.actor_id=response.actor_id JOIN outbox_events outbox ON outbox.command_id=receipt.command_id AND outbox.event_type=receipt.operation AND outbox.aggregate_id=binding.id AND outbox.aggregate_version=response.response_version WHERE receipt.command_id=response.command_id AND receipt.record_id=binding.id AND receipt.operation='support.ticket-portal.respond' AND receipt.actor_id=response.actor_id AND receipt.authority_json::jsonb->>'companyId'=binding.company_id AND receipt.result_json::jsonb->>'id'=response.id AND receipt.result_json::jsonb->>'bindingId'=binding.id AND receipt.result_json::jsonb->>'version'=response.response_version::text AND receipt.result_json::jsonb->>'message'=response.message AND receipt.result_json::jsonb=jsonb_build_object('id',response.id,'bindingId',response.binding_id,'version',response.response_version,'status','published','message',response.message,'publishedAt',response.created_at))))
 OR EXISTS(SELECT 1 FROM support_ticket_portal_acknowledgements ack LEFT JOIN support_ticket_portal_identities identity ON identity.binding_id=ack.binding_id AND identity.user_id=ack.user_id AND identity.email=ack.email LEFT JOIN support_ticket_portal_responses response ON response.id=ack.response_id AND response.binding_id=ack.binding_id AND response.response_version=ack.response_version WHERE ack.binding_id=binding.id AND (identity.binding_id IS NULL OR response.id IS NULL OR ack.company_id<>binding.company_id OR ack.binding_version>binding.version OR NOT EXISTS(SELECT 1 FROM audit_events audit JOIN outbox_events event ON event.command_id=audit.command_id AND event.event_type=audit.action AND event.aggregate_id=binding.id AND event.aggregate_version=ack.response_version WHERE audit.command_id=ack.request_key AND audit.actor_id=ack.user_id AND audit.assignment_id='external-customer' AND audit.action='support.ticket-portal.acknowledge' AND audit.record_id=binding.id AND audit.record_version=ack.response_version AND audit.authority_json::jsonb->>'bindingId'=binding.id AND audit.authority_json::jsonb->>'email'=ack.email AND event.payload_json::jsonb->>'bindingId'=binding.id AND event.payload_json::jsonb->>'responseId'=ack.response_id AND event.payload_json::jsonb->>'userId'=ack.user_id)))
 THEN RAISE EXCEPTION 'SUPPORT_PORTAL_SOURCE_CHANGED' USING ERRCODE='23514';END IF;
END $$;
-- Both directions are required: a valid old binding cannot justify a new
-- orphan publication/decision receipt with no actual native child.
CREATE FUNCTION support_ticket_portal_assert_command(command_key text,command_operation text,binding_key text) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 IF command_operation IN('support.ticket-portal.prepare','support.ticket-portal.approve','support.ticket-portal.reject','support.ticket-portal.revoke') THEN
  IF NOT EXISTS(SELECT 1 FROM support_ticket_portal_events event JOIN command_receipts receipt ON receipt.command_id=event.command_id AND receipt.record_id=event.binding_id AND receipt.actor_id=event.actor_id AND receipt.operation='support.ticket-portal.'||event.action AND receipt.authority_json::jsonb->>'companyId'=event.company_id WHERE event.command_id=command_key AND event.binding_id=binding_key AND receipt.operation=command_operation AND receipt.result_json::jsonb=jsonb_build_object('id',event.binding_id,'status',CASE event.action WHEN 'prepare' THEN 'draft' WHEN 'approve' THEN 'approved' WHEN 'reject' THEN 'rejected' ELSE 'revoked' END,'version',event.version)) THEN RAISE EXCEPTION 'SUPPORT_PORTAL_SOURCE_CHANGED' USING ERRCODE='23514';END IF;
 ELSIF command_operation='support.ticket-portal.respond' THEN
  IF NOT EXISTS(SELECT 1 FROM support_ticket_portal_responses response JOIN command_receipts receipt ON receipt.command_id=response.command_id AND receipt.record_id=response.binding_id AND receipt.actor_id=response.actor_id AND receipt.operation=command_operation AND receipt.authority_json::jsonb->>'companyId'=response.company_id WHERE response.command_id=command_key AND response.binding_id=binding_key AND receipt.result_json::jsonb=jsonb_build_object('id',response.id,'bindingId',response.binding_id,'version',response.response_version,'status','published','message',response.message,'publishedAt',response.created_at)) THEN RAISE EXCEPTION 'SUPPORT_PORTAL_SOURCE_CHANGED' USING ERRCODE='23514';END IF;
 ELSIF command_operation='support.ticket-portal.acknowledge' THEN
  IF NOT EXISTS(SELECT 1 FROM support_ticket_portal_acknowledgements ack WHERE ack.request_key=command_key AND ack.binding_id=binding_key) THEN RAISE EXCEPTION 'SUPPORT_PORTAL_SOURCE_CHANGED' USING ERRCODE='23514';END IF;
 ELSE RAISE EXCEPTION 'SUPPORT_PORTAL_SOURCE_CHANGED' USING ERRCODE='23514';
 END IF;
 PERFORM support_ticket_portal_assert(binding_key);
END $$;
CREATE FUNCTION support_ticket_portal_reciprocal() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE binding_key text;
BEGIN
 IF TG_ARGV[0]='receipt' THEN
  IF TG_OP<>'DELETE' AND NEW.operation='support.ticket-portal.acknowledge' THEN RAISE EXCEPTION 'SUPPORT_PORTAL_SOURCE_CHANGED' USING ERRCODE='23514';END IF;
  IF TG_OP<>'INSERT' AND OLD.operation LIKE 'support.ticket-portal.%' THEN PERFORM support_ticket_portal_assert_command(OLD.command_id,OLD.operation,OLD.record_id);END IF;
  IF TG_OP<>'DELETE' AND NEW.operation LIKE 'support.ticket-portal.%' THEN PERFORM support_ticket_portal_assert_command(NEW.command_id,NEW.operation,NEW.record_id);END IF;
 ELSIF TG_ARGV[0]='audit' THEN
  IF TG_OP<>'INSERT' AND OLD.action LIKE 'support.ticket-portal.%' THEN PERFORM support_ticket_portal_assert_command(OLD.command_id,OLD.action,OLD.record_id);END IF;
  IF TG_OP<>'DELETE' AND NEW.action LIKE 'support.ticket-portal.%' THEN PERFORM support_ticket_portal_assert_command(NEW.command_id,NEW.action,NEW.record_id);END IF;
 ELSIF TG_ARGV[0]='outbox' THEN
  IF TG_OP<>'INSERT' AND OLD.event_type LIKE 'support.ticket-portal.%' THEN PERFORM support_ticket_portal_assert_command(OLD.command_id,OLD.event_type,OLD.aggregate_id);END IF;
  IF TG_OP<>'DELETE' AND NEW.event_type LIKE 'support.ticket-portal.%' THEN PERFORM support_ticket_portal_assert_command(NEW.command_id,NEW.event_type,NEW.aggregate_id);END IF;
 ELSIF TG_ARGV[0]='ticket' THEN
  FOR binding_key IN SELECT binding.id FROM support_ticket_portal_bindings binding WHERE binding.ticket_id=COALESCE(NEW.id,OLD.id) LOOP PERFORM support_ticket_portal_assert(binding_key);END LOOP;
 ELSIF TG_ARGV[0]='binding' THEN PERFORM support_ticket_portal_assert(COALESCE(NEW.id,OLD.id));
 ELSE PERFORM support_ticket_portal_assert(COALESCE(NEW.binding_id,OLD.binding_id));
 END IF;
 RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER support_ticket_portal_binding_proof AFTER INSERT OR UPDATE OR DELETE ON support_ticket_portal_bindings DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION support_ticket_portal_reciprocal('binding');
CREATE CONSTRAINT TRIGGER support_ticket_portal_event_proof AFTER INSERT OR UPDATE OR DELETE ON support_ticket_portal_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION support_ticket_portal_reciprocal('child');
CREATE CONSTRAINT TRIGGER support_ticket_portal_identity_proof AFTER INSERT OR UPDATE OR DELETE ON support_ticket_portal_identities DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION support_ticket_portal_reciprocal('child');
CREATE CONSTRAINT TRIGGER support_ticket_portal_read_proof AFTER INSERT OR UPDATE OR DELETE ON support_ticket_portal_reads DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION support_ticket_portal_reciprocal('child');
CREATE CONSTRAINT TRIGGER support_ticket_portal_response_proof AFTER INSERT OR UPDATE OR DELETE ON support_ticket_portal_responses DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION support_ticket_portal_reciprocal('child');
CREATE CONSTRAINT TRIGGER support_ticket_portal_ack_proof AFTER INSERT OR UPDATE OR DELETE ON support_ticket_portal_acknowledgements DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION support_ticket_portal_reciprocal('child');
CREATE CONSTRAINT TRIGGER support_ticket_portal_ticket_proof AFTER UPDATE OR DELETE ON support_tickets DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION support_ticket_portal_reciprocal('ticket');
CREATE CONSTRAINT TRIGGER support_ticket_portal_receipt_proof AFTER INSERT OR UPDATE OR DELETE ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION support_ticket_portal_reciprocal('receipt');
CREATE CONSTRAINT TRIGGER support_ticket_portal_audit_proof AFTER INSERT OR UPDATE OR DELETE ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION support_ticket_portal_reciprocal('audit');
CREATE CONSTRAINT TRIGGER support_ticket_portal_outbox_proof AFTER INSERT OR UPDATE OR DELETE ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION support_ticket_portal_reciprocal('outbox');
-- A native Portal command is born as that operation. Original authority
-- capture may enrich its receipt but cannot convert an older unrelated command.
CREATE FUNCTION support_ticket_portal_receipt_birth() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF OLD.operation LIKE 'support.ticket-portal.%' OR NEW.operation LIKE 'support.ticket-portal.%' THEN
  IF OLD.operation IS DISTINCT FROM NEW.operation OR OLD.command_id IS DISTINCT FROM NEW.command_id OR OLD.record_id IS DISTINCT FROM NEW.record_id OR OLD.actor_id IS DISTINCT FROM NEW.actor_id OR OLD.request_hash IS DISTINCT FROM NEW.request_hash OR OLD.result_json IS DISTINCT FROM NEW.result_json THEN RAISE EXCEPTION 'SUPPORT_PORTAL_SOURCE_CHANGED' USING ERRCODE='23514';END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER support_ticket_portal_receipt_birth BEFORE UPDATE ON command_receipts FOR EACH ROW EXECUTE FUNCTION support_ticket_portal_receipt_birth();
