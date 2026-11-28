-- Field invoice responsibilities have one writer. Shared invoice defaults remain
-- append-only Workflow policy also consumed by non-Field Finance; this migration
-- removes no data and rewrites no previous route/event/receipt/history.
CREATE FUNCTION erpx_finance_field_invoice_owned(invoice_ref text,company_ref text)
RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM finance_invoices i WHERE i.id=invoice_ref AND (company_ref IS NULL OR i.company_id=company_ref) AND i.source_type='field_service')
 OR EXISTS(SELECT 1 FROM money_transaction_snapshots m WHERE (company_ref IS NULL OR m.company_id=company_ref) AND m.source_type='finance_invoice' AND m.source_id=invoice_ref AND m.carried_source_type='field_service_billing_release')
$$;
CREATE FUNCTION erpx_service_invoice_flow_marker() RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE raw text; marker jsonb;
BEGIN
 raw:=current_setting('erpx.service_invoice_flow',true);
 IF raw IS JSON OBJECT THEN marker:=raw::jsonb; ELSE RETURN '{}'::jsonb; END IF;
 IF marker->>'database' IS DISTINCT FROM current_database() OR marker->>'pid' IS DISTINCT FROM pg_backend_pid()::text
 OR marker->>'xid' IS DISTINCT FROM pg_current_xact_id()::text OR marker->>'started' IS DISTINCT FROM pg_postmaster_start_time()::text THEN RETURN '{}'::jsonb; END IF;
 RETURN marker;
END $$;
CREATE FUNCTION erpx_guard_native_service_invoice_flow_route() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE actual jsonb;
BEGIN
 IF (TG_OP<>'INSERT' AND OLD.kind='invoice' AND erpx_finance_field_invoice_owned(OLD.record_id,OLD.company_id))
 OR (TG_OP<>'DELETE' AND NEW.kind='invoice' AND erpx_finance_field_invoice_owned(NEW.record_id,NEW.company_id)) THEN
  IF TG_OP='DELETE' THEN RAISE EXCEPTION 'Field invoice routing requires the native owner unit' USING ERRCODE='23514',CONSTRAINT='native_service_invoice_flow_required'; END IF;
  SELECT to_jsonb(r) INTO actual FROM flow_routes r WHERE r.company_id=NEW.company_id AND r.kind=NEW.kind AND r.record_id=NEW.record_id;
  IF actual IS NULL OR NEW.kind<>'invoice' OR NOT erpx_finance_field_invoice_owned(NEW.record_id,NEW.company_id) OR erpx_service_invoice_flow_marker()->'route' IS DISTINCT FROM actual THEN
   RAISE EXCEPTION 'Field invoice routing requires the native owner unit' USING ERRCODE='23514',CONSTRAINT='native_service_invoice_flow_required';
  END IF;
 END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; END IF; RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER native_service_invoice_flow_route_required AFTER INSERT OR UPDATE OR DELETE ON flow_routes
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION erpx_guard_native_service_invoice_flow_route();
CREATE FUNCTION erpx_guard_native_service_invoice_flow_event() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE actual jsonb;
BEGIN
 IF (TG_OP<>'INSERT' AND OLD.kind='invoice' AND erpx_finance_field_invoice_owned(OLD.record_id,OLD.company_id))
 OR (TG_OP<>'DELETE' AND NEW.kind='invoice' AND erpx_finance_field_invoice_owned(NEW.record_id,NEW.company_id)) THEN
  IF TG_OP<>'INSERT' THEN RAISE EXCEPTION 'Field invoice Flow history is retained' USING ERRCODE='23514',CONSTRAINT='native_service_invoice_flow_required'; END IF;
  SELECT to_jsonb(e) INTO actual FROM flow_events e WHERE e.id=NEW.id;
  IF actual IS NULL OR erpx_service_invoice_flow_marker()->'event' IS DISTINCT FROM actual THEN
   RAISE EXCEPTION 'Field invoice Flow requires the native owner unit' USING ERRCODE='23514',CONSTRAINT='native_service_invoice_flow_required';
  END IF;
 END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; END IF; RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER native_service_invoice_flow_event_required AFTER INSERT OR UPDATE OR DELETE ON flow_events
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION erpx_guard_native_service_invoice_flow_event();
CREATE FUNCTION erpx_guard_native_service_invoice_flow_notice() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE marker jsonb; raw text; actual jsonb;
BEGIN
 IF (TG_OP<>'INSERT' AND OLD.record_type='invoice' AND erpx_finance_field_invoice_owned(OLD.record_id,OLD.company_id))
 OR (TG_OP<>'DELETE' AND NEW.record_type='invoice' AND erpx_finance_field_invoice_owned(NEW.record_id,NEW.company_id)) THEN
  IF TG_OP='DELETE' THEN RAISE EXCEPTION 'Field invoice notifications are retained' USING ERRCODE='23514',CONSTRAINT='native_service_invoice_notice_required'; END IF;
  raw:=current_setting('erpx.service_invoice_flow_notice',true);
  IF raw IS JSON OBJECT THEN marker:=raw::jsonb; ELSE marker:='{}'::jsonb; END IF;
  SELECT to_jsonb(n) INTO actual FROM staff_notifications n WHERE n.id=NEW.id;
  IF marker->>'database' IS DISTINCT FROM current_database() OR marker->>'pid' IS DISTINCT FROM pg_backend_pid()::text
   OR marker->>'xid' IS DISTINCT FROM pg_current_xact_id()::text OR marker->>'started' IS DISTINCT FROM pg_postmaster_start_time()::text
   OR actual IS NULL OR NEW.record_type<>'invoice' OR NOT erpx_finance_field_invoice_owned(NEW.record_id,NEW.company_id)
   OR marker->'notice' IS DISTINCT FROM actual THEN
   RAISE EXCEPTION 'Field invoice notifications require the native owner unit' USING ERRCODE='23514',CONSTRAINT='native_service_invoice_notice_required';
  END IF;
 END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; END IF; RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER native_service_invoice_notice_required AFTER INSERT OR UPDATE OF id,company_id,user_id,record_type,record_id,record_version,message,created_at OR DELETE ON staff_notifications
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION erpx_guard_native_service_invoice_flow_notice();
CREATE OR REPLACE FUNCTION erpx_field_push_source(source_type text,source_id text,company text)
RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT coalesce($1='field_service_order'
 OR ($1='stock_count' AND EXISTS(SELECT 1 FROM inventory_service_initial_physical_sources s WHERE s.count_id=$2 AND s.company_id=$3))
 OR ($1='operational_exception' AND EXISTS(SELECT 1 FROM operations_exceptions s WHERE s.id=$2 AND s.company_id=$3 AND s.source_kind='field_service_order'))
 OR ($1='collaboration_action' AND EXISTS(SELECT 1 FROM collaboration_action_cards s WHERE s.id=$2 AND s.company_id=$3 AND s.source_type='field_service_order'))
 OR ($1='support_ticket' AND EXISTS(SELECT 1 FROM support_tickets s WHERE s.id=$2 AND s.company_id=$3 AND s.source_kind='field_service'))
 OR ($1='invoice' AND erpx_finance_field_invoice_owned($2,$3)),false)
$$;

CREATE OR REPLACE FUNCTION erpx_require_native_field_inbox_state() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF current_setting('erpx.inbox_state_runtime',true) IS DISTINCT FROM 'native-rust'
 AND (NEW.read_at IS DISTINCT FROM OLD.read_at OR NEW.archived_at IS DISTINCT FROM OLD.archived_at)
 AND EXISTS(SELECT 1 FROM inbox_events e WHERE e.source_type IN('field_service_order','operational_exception','collaboration_action','support_ticket','stock_count','invoice') AND erpx_field_push_source(e.source_type,e.source_id,e.company_id) AND ((e.id=OLD.event_id AND e.company_id=OLD.company_id) OR (e.id=NEW.event_id AND e.company_id=NEW.company_id))) THEN
  RAISE EXCEPTION 'Field Inbox state requires the native runtime' USING ERRCODE='23514',CONSTRAINT='native_field_inbox_state_required';
 END IF;
 RETURN NEW;
END $$;
CREATE OR REPLACE FUNCTION erpx_require_native_field_staff_state() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF current_setting('erpx.inbox_state_runtime',true) IS DISTINCT FROM 'native-rust' AND NEW.read_at IS DISTINCT FROM OLD.read_at AND ((OLD.record_type IN('field_service_order','operational_exception','collaboration_action','support_ticket','stock_count','invoice') AND erpx_field_push_source(OLD.record_type,OLD.record_id,OLD.company_id)) OR (NEW.record_type IN('field_service_order','operational_exception','collaboration_action','support_ticket','stock_count','invoice') AND erpx_field_push_source(NEW.record_type,NEW.record_id,NEW.company_id))) THEN
  RAISE EXCEPTION 'Field notification state requires the native runtime' USING ERRCODE='23514',CONSTRAINT='native_field_staff_state_required';
 END IF;
 RETURN NEW;
END $$;


CREATE OR REPLACE FUNCTION erpx_native_outbox_owned(candidate outbox_events) RETURNS boolean
LANGUAGE sql STABLE AS $$
SELECT coalesce((
((
  (e.aggregate_type='inventory_count' AND e.event_type LIKE 'inventory.count.%'
   OR e.aggregate_type='flow_assign' AND e.event_type='flow.assign')
  AND (
    EXISTS (SELECT 1 FROM inventory_service_initial_physical_sources s WHERE s.count_id=e.aggregate_id)
    OR EXISTS (SELECT 1 FROM command_receipts r WHERE r.operation='inventory.count.create' AND r.record_id=e.aggregate_id
      AND (r.result_json IS NULL OR jsonb_typeof(r.result_json::jsonb)<>'object' OR jsonb_exists(r.result_json::jsonb,'_serviceInitialPhysicalStockCount')))
  )
  AND NOT (
    e.aggregate_type='flow_assign'
    AND (SELECT count(*) FROM flow_events all_events WHERE all_events.command_id=e.command_id)=1
    AND EXISTS (SELECT 1 FROM flow_events fe JOIN command_receipts fr ON fr.command_id=fe.command_id
      WHERE fe.command_id=e.command_id AND fe.record_id=e.aggregate_id AND fe.record_version=e.aggregate_version AND e.payload_json::jsonb->>'id'=e.aggregate_id AND e.payload_json::jsonb->'version'=to_jsonb(e.aggregate_version) AND fe.action='assign'
        AND fe.kind IN ('expense','leave','staff_task','request','transfer','order','invoice','journal','payroll','record')
        AND fe.company_id=e.payload_json::jsonb->>'companyId' AND fe.actor_user_id=fr.actor_id
        AND fr.operation='flow.assign' AND fr.record_id=e.aggregate_id
        AND (CASE WHEN fr.authority_json IS JSON OBJECT THEN fr.authority_json::jsonb ELSE '{}'::jsonb END)->>'companyId'=fe.company_id)
  )
)) OR
((e.event_type LIKE 'field.order.%' OR e.event_type LIKE 'field.agreement.%')) OR
(((e.event_type LIKE 'operations.exception.%' AND EXISTS(SELECT 1 FROM operations_exceptions s WHERE s.id=e.aggregate_id AND s.source_kind='field_service_order')) OR (e.event_type LIKE 'operations.policy.%' AND EXISTS(SELECT 1 FROM operations_queue_policies s WHERE s.id=e.aggregate_id AND jsonb_typeof(s.source_kinds_json::jsonb)='array' AND s.source_kinds_json::jsonb<>'[]'::jsonb AND s.source_kinds_json::jsonb <@ '["field_service_order"]'::jsonb)))) OR
((((e.event_type LIKE 'collaboration.action.%' AND EXISTS(SELECT 1 FROM collaboration_action_cards s WHERE s.id=e.aggregate_id AND s.source_type='field_service_order')) OR (e.event_type LIKE 'collaboration.relation.%' AND EXISTS(SELECT 1 FROM collaboration_record_relations s WHERE s.id=e.aggregate_id AND ((s.source_type='field_service_order' AND s.target_type IN ('field_service_order','support_ticket')) OR (s.source_type='support_ticket' AND s.target_type='field_service_order'))))) OR (e.event_type='inbox.delivery.state' AND (EXISTS(SELECT 1 FROM inbox_deliveries d JOIN inbox_events n ON n.id=d.event_id AND n.company_id=d.company_id WHERE d.id=e.aggregate_id AND n.source_type IN('field_service_order','operational_exception','collaboration_action','support_ticket','stock_count','invoice') AND erpx_field_push_source(n.source_type,n.source_id,n.company_id)) OR EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=e.command_id AND (CASE WHEN r.result_json IS JSON OBJECT THEN r.result_json::jsonb ELSE '{}'::jsonb END)->'_nativeFieldInbox'='true'::jsonb))))) OR
(e.event_type IN ('attachment.set.create','attachment.record.bind')
AND EXISTS(SELECT 1 FROM attachment_sets s WHERE erpx_files_native_target_owned(s.target_type,s.target_id)
 AND (s.bound_command_id=e.command_id
  OR EXISTS(SELECT 1 FROM attachment_events origin WHERE origin.set_id=s.id AND origin.action='set.native-origin' AND origin.details_json::jsonb->>'commandId'=e.command_id)
  OR EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=e.command_id AND (CASE WHEN r.result_json IS JSON OBJECT THEN r.result_json::jsonb ELSE '{}'::jsonb END)->>'setId'=s.id))))
 OR
((((e.event_type='finance.party.create' AND EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=e.command_id AND jsonb_exists(CASE WHEN r.result_json IS JSON OBJECT THEN r.result_json::jsonb ELSE '{}'::jsonb END,'_serviceCustomerPreparation'))) OR (e.event_type='finance.invoice.post' AND EXISTS(SELECT 1 FROM finance_invoices i WHERE i.id=e.aggregate_id AND i.source_type='field_service'))) OR ((e.aggregate_type='finance_payment' AND e.event_type IN('finance.payment.create','finance.payment.approve','finance.payment.release','finance.payment.confirm') AND erpx_finance_field_payment_owned(e.aggregate_id)) OR (e.aggregate_type='finance_bank' AND e.event_type='finance.bank.import' AND erpx_finance_field_statement_owned(e.aggregate_id)))) OR (e.aggregate_type='flow_assign' AND e.event_type='flow.assign' AND erpx_finance_field_invoice_owned(e.aggregate_id,NULL)))
),false) FROM (SELECT (candidate).*) e
$$;
