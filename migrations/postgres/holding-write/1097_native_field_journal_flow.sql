-- Native Field-parts journal responsibility writer fence. Other journal owners
-- and shared defaults remain intact. No data, schema history or receipts removed.
CREATE FUNCTION erpx_finance_field_journal_owned(journal_ref text,company_ref text)
RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM finance_journals j WHERE j.id=$1 AND ($2 IS NULL OR j.company_id=$2) AND j.source_type='field_service_parts')
 OR EXISTS(SELECT 1 FROM operational_inventory_consumptions p WHERE p.journal_id=$1 AND ($2 IS NULL OR p.company_id=$2) AND p.source_kind='field_service')
$$;
-- Existing scan, cleanup and legacy-writer fences consume this owning
-- predicate; native journal operations also validate Company and full lineage.
CREATE OR REPLACE FUNCTION erpx_files_native_target_owned(target_kind text,target_ref text)
RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT coalesce(target_kind='field_service_visit'
 OR (target_kind='finance_payment' AND erpx_finance_field_payment_owned(target_ref))
 OR (target_kind='finance_journal' AND erpx_finance_field_journal_owned(target_ref,NULL)),false)
$$;
CREATE FUNCTION erpx_field_journal_flow_marker() RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE raw text; marker jsonb;
BEGIN
 raw:=current_setting('erpx.field_journal_flow',true);
 IF raw IS JSON OBJECT THEN marker:=raw::jsonb; ELSE RETURN '{}'::jsonb; END IF;
 IF marker->>'database' IS DISTINCT FROM current_database() OR marker->>'pid' IS DISTINCT FROM pg_backend_pid()::text
 OR marker->>'xid' IS DISTINCT FROM pg_current_xact_id()::text OR marker->>'started' IS DISTINCT FROM pg_postmaster_start_time()::text THEN RETURN '{}'::jsonb; END IF;
 RETURN marker;
END $$;
CREATE FUNCTION erpx_guard_native_field_journal_flow_route() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE actual jsonb;
BEGIN
 IF (TG_OP<>'INSERT' AND OLD.kind='journal' AND erpx_finance_field_journal_owned(OLD.record_id,OLD.company_id))
 OR (TG_OP<>'DELETE' AND NEW.kind='journal' AND erpx_finance_field_journal_owned(NEW.record_id,NEW.company_id)) THEN
  IF TG_OP='DELETE' THEN RAISE EXCEPTION 'Field journal routing requires the native owner unit' USING ERRCODE='23514',CONSTRAINT='native_field_journal_flow_required'; END IF;
  SELECT to_jsonb(r) INTO actual FROM flow_routes r WHERE r.company_id=NEW.company_id AND r.kind=NEW.kind AND r.record_id=NEW.record_id;
  IF actual IS NULL OR NEW.kind<>'journal' OR NOT erpx_finance_field_journal_owned(NEW.record_id,NEW.company_id)
  OR erpx_field_journal_flow_marker()->'route' IS DISTINCT FROM actual THEN
   RAISE EXCEPTION 'Field journal routing requires the native owner unit' USING ERRCODE='23514',CONSTRAINT='native_field_journal_flow_required';
  END IF;
 END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; END IF; RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER native_field_journal_flow_route_required AFTER INSERT OR UPDATE OR DELETE ON flow_routes
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION erpx_guard_native_field_journal_flow_route();
CREATE FUNCTION erpx_guard_native_field_journal_flow_event() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE actual jsonb;
BEGIN
 IF (TG_OP<>'INSERT' AND OLD.kind='journal' AND erpx_finance_field_journal_owned(OLD.record_id,OLD.company_id))
 OR (TG_OP<>'DELETE' AND NEW.kind='journal' AND erpx_finance_field_journal_owned(NEW.record_id,NEW.company_id)) THEN
  IF TG_OP='DELETE' THEN RAISE EXCEPTION 'Field journal Flow history is retained' USING ERRCODE='23514',CONSTRAINT='native_field_journal_flow_required'; END IF;
  -- The existing native post protocol fills next-step metadata in the same unit.
  -- Assignment and all historical event facts are otherwise immutable.
  IF TG_OP='UPDATE' AND (OLD.action<>'post' OR NEW.action<>'post'
   OR (to_jsonb(OLD)-'next_step_key'-'next_user_id') IS DISTINCT FROM (to_jsonb(NEW)-'next_step_key'-'next_user_id')) THEN
   RAISE EXCEPTION 'Field journal Flow history is retained' USING ERRCODE='23514',CONSTRAINT='native_field_journal_flow_required';
  END IF;
  SELECT to_jsonb(e) INTO actual FROM flow_events e WHERE e.id=NEW.id;
  IF actual IS NULL OR NEW.kind<>'journal' OR NOT erpx_finance_field_journal_owned(NEW.record_id,NEW.company_id)
   OR erpx_field_journal_flow_marker()->'event' IS DISTINCT FROM actual THEN
   RAISE EXCEPTION 'Field journal Flow requires the native owner unit' USING ERRCODE='23514',CONSTRAINT='native_field_journal_flow_required';
  END IF;
 END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; END IF; RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER native_field_journal_flow_event_required AFTER INSERT OR UPDATE OR DELETE ON flow_events
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION erpx_guard_native_field_journal_flow_event();
CREATE FUNCTION erpx_guard_native_field_journal_flow_notice() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE marker jsonb; raw text; actual jsonb;
BEGIN
 IF (TG_OP<>'INSERT' AND OLD.record_type='journal' AND erpx_finance_field_journal_owned(OLD.record_id,OLD.company_id))
 OR (TG_OP<>'DELETE' AND NEW.record_type='journal' AND erpx_finance_field_journal_owned(NEW.record_id,NEW.company_id)) THEN
  IF TG_OP='DELETE' THEN RAISE EXCEPTION 'Field journal notifications are retained' USING ERRCODE='23514',CONSTRAINT='native_field_journal_notice_required'; END IF;
  raw:=current_setting('erpx.field_journal_flow_notice',true);
  IF raw IS JSON OBJECT THEN marker:=raw::jsonb; ELSE marker:='{}'::jsonb; END IF;
  SELECT to_jsonb(n) INTO actual FROM staff_notifications n WHERE n.id=NEW.id;
  IF marker->>'database' IS DISTINCT FROM current_database() OR marker->>'pid' IS DISTINCT FROM pg_backend_pid()::text
   OR marker->>'xid' IS DISTINCT FROM pg_current_xact_id()::text OR marker->>'started' IS DISTINCT FROM pg_postmaster_start_time()::text
   OR actual IS NULL OR NEW.record_type<>'journal' OR NOT erpx_finance_field_journal_owned(NEW.record_id,NEW.company_id)
   OR marker->'notice' IS DISTINCT FROM actual THEN
   RAISE EXCEPTION 'Field journal notifications require the native owner unit' USING ERRCODE='23514',CONSTRAINT='native_field_journal_notice_required';
  END IF;
 END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; END IF; RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER native_field_journal_notice_required AFTER INSERT OR UPDATE OF id,company_id,user_id,record_type,record_id,record_version,message,created_at OR DELETE ON staff_notifications
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION erpx_guard_native_field_journal_flow_notice();

-- Keep the finite cross-owner proof out of the shared multi-owner lease plan.
-- These are historical committed facts; current IAM/purchase changes do not
-- rewrite a completed independent decision or its queued read projection.
CREATE FUNCTION erpx_native_field_journal_outbox_eligible(candidate outbox_events)
RETURNS boolean LANGUAGE plpgsql STABLE AS $$
DECLARE e outbox_events:=candidate;
BEGIN
 IF e.event_type NOT IN('flow.assign','finance.journal.post')
 OR NOT (e.payload_json IS JSON OBJECT) THEN RETURN false; END IF;
 RETURN coalesce((
  e.aggregate_type=CASE e.event_type WHEN 'flow.assign' THEN 'flow_assign' ELSE 'finance_journal' END
  AND e.aggregate_version BETWEEN 1 AND 9007199254740991
  AND e.payload_json::jsonb->>'id'=e.aggregate_id
  AND e.payload_json::jsonb->'version'=to_jsonb(e.aggregate_version)
  AND (SELECT count(*) FROM outbox_events all_events WHERE all_events.command_id=e.command_id)=1
  AND EXISTS(
   SELECT 1 FROM command_receipts r
   JOIN finance_journals j ON j.id=r.record_id AND j.company_id=r.authority_json::jsonb->>'companyId'
   JOIN operational_inventory_consumptions p ON p.journal_id=j.id AND p.id=j.source_id AND p.company_id=j.company_id AND p.source_kind='field_service'
   JOIN command_receipts issue ON p.id='operational-part-'||issue.command_id AND issue.operation='field.order.issue-part'
    AND issue.record_id=p.source_id AND issue.actor_id=p.created_by
    AND issue.authority_json::jsonb->>'companyId'=p.company_id
    -- Holding is the validated worker's physical database binding; the
    -- original Workflow authority wire stores Company, not holdingId.
   JOIN flow_events f ON f.command_id=r.command_id AND f.company_id=j.company_id AND f.kind='journal' AND f.record_id=j.id
   WHERE r.command_id=e.command_id AND r.operation=e.event_type AND r.record_id=e.aggregate_id
    AND j.source_type='field_service_parts' AND j.version>=e.aggregate_version
    AND j.company_id=e.payload_json::jsonb->>'companyId' AND j.currency=p.currency
    AND p.value_minor BETWEEN 1 AND 9007199254740991
    AND EXISTS(SELECT 1 FROM money_transaction_snapshots m WHERE m.company_id=p.company_id
     AND m.source_type='operational_inventory_consumption' AND m.source_id=p.id
     AND m.amount_kind='stock_cost' AND m.currency=p.currency AND m.amount_units=p.value_minor)
    AND (SELECT sum(l.debit_minor)::numeric FROM finance_journal_lines l WHERE l.journal_id=j.id)=p.value_minor
    AND (SELECT sum(l.credit_minor)::numeric FROM finance_journal_lines l WHERE l.journal_id=j.id)=p.value_minor
    AND r.result_json::jsonb->>'id'=j.id
    AND f.actor_user_id=r.actor_id AND f.record_version=e.aggregate_version
    AND f.created_at BETWEEN 0 AND 9007199254740991
    AND (e.event_type<>'flow.assign' OR f.created_at=r.created_at)
    AND (SELECT count(*) FROM flow_events all_events WHERE all_events.command_id=r.command_id)=1
    AND (
     (e.event_type='flow.assign' AND f.action='assign'
      AND CASE WHEN jsonb_typeof(r.result_json::jsonb->'version')='number' THEN (r.result_json::jsonb->>'version')::numeric END BETWEEN 1 AND 9007199254740991
      AND EXISTS(SELECT 1 FROM staff_notifications n WHERE n.id='field-journal-flow-notice-'||r.command_id
       AND n.company_id=j.company_id AND n.record_type='journal' AND n.record_id=j.id
       AND n.record_version=e.aggregate_version AND n.created_at=r.created_at))
     OR (e.event_type='finance.journal.post' AND f.action='post'
      AND f.next_step_key='posted' AND f.next_user_id IS NULL
      AND j.status='posted' AND e.payload_json::jsonb->>'state'='posted'
      AND r.result_json::jsonb->>'status'='posted'
      AND r.result_json::jsonb->'version'=to_jsonb(e.aggregate_version)))
    AND (SELECT count(*) FROM audit_events a WHERE a.command_id=r.command_id)=1
    AND EXISTS(SELECT 1 FROM audit_events a WHERE a.command_id=r.command_id AND a.actor_id=r.actor_id
     AND a.action=e.event_type AND a.record_type=e.aggregate_type AND a.record_id=j.id
     AND a.record_version=e.aggregate_version AND a.authority_json=r.authority_json
     AND a.created_at=r.created_at AND e.created_at=r.created_at)
  )
 ),false);
END $$;


CREATE OR REPLACE FUNCTION erpx_field_push_source(source_type text,source_id text,company text)
RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT coalesce($1='field_service_order'
 OR ($1='stock_count' AND EXISTS(SELECT 1 FROM inventory_service_initial_physical_sources s WHERE s.count_id=$2 AND s.company_id=$3))
 OR ($1='operational_exception' AND EXISTS(SELECT 1 FROM operations_exceptions s WHERE s.id=$2 AND s.company_id=$3 AND s.source_kind='field_service_order'))
 OR ($1='collaboration_action' AND EXISTS(SELECT 1 FROM collaboration_action_cards s WHERE s.id=$2 AND s.company_id=$3 AND s.source_type='field_service_order'))
 OR ($1='support_ticket' AND EXISTS(SELECT 1 FROM support_tickets s WHERE s.id=$2 AND s.company_id=$3 AND s.source_kind='field_service'))
 OR ($1='journal' AND erpx_finance_field_journal_owned($2,$3))
 OR ($1='invoice' AND erpx_finance_field_invoice_owned($2,$3)),false)
$$;

CREATE OR REPLACE FUNCTION erpx_require_native_field_inbox_state() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF current_setting('erpx.inbox_state_runtime',true) IS DISTINCT FROM 'native-rust'
 AND (NEW.read_at IS DISTINCT FROM OLD.read_at OR NEW.archived_at IS DISTINCT FROM OLD.archived_at)
 AND EXISTS(SELECT 1 FROM inbox_events e WHERE e.source_type IN('field_service_order','operational_exception','collaboration_action','support_ticket','stock_count','invoice','journal') AND erpx_field_push_source(e.source_type,e.source_id,e.company_id) AND ((e.id=OLD.event_id AND e.company_id=OLD.company_id) OR (e.id=NEW.event_id AND e.company_id=NEW.company_id))) THEN
  RAISE EXCEPTION 'Field Inbox state requires the native runtime' USING ERRCODE='23514',CONSTRAINT='native_field_inbox_state_required';
 END IF;
 RETURN NEW;
END $$;
CREATE OR REPLACE FUNCTION erpx_require_native_field_staff_state() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF current_setting('erpx.inbox_state_runtime',true) IS DISTINCT FROM 'native-rust' AND NEW.read_at IS DISTINCT FROM OLD.read_at AND ((OLD.record_type IN('field_service_order','operational_exception','collaboration_action','support_ticket','stock_count','invoice','journal') AND erpx_field_push_source(OLD.record_type,OLD.record_id,OLD.company_id)) OR (NEW.record_type IN('field_service_order','operational_exception','collaboration_action','support_ticket','stock_count','invoice','journal') AND erpx_field_push_source(NEW.record_type,NEW.record_id,NEW.company_id))) THEN
  RAISE EXCEPTION 'Field notification state requires the native runtime' USING ERRCODE='23514',CONSTRAINT='native_field_staff_state_required';
 END IF;
 RETURN NEW;
END $$;



-- Preserve each existing Finance eligibility assertion while planning only the
-- actual event family. Inlining the combined disjunction can exhaust the worker
-- database during planning before any row is examined.
CREATE FUNCTION erpx_native_finance_field_outbox_eligible(candidate outbox_events)
RETURNS boolean LANGUAGE plpgsql STABLE AS $$
DECLARE e outbox_events:=candidate;
BEGIN
 IF e.event_type='finance.party.create' THEN
  IF coalesce((
(e.aggregate_type='finance_party' AND e.event_type='finance.party.create' AND e.aggregate_version=1
AND e.payload_json::jsonb->>'id'=e.aggregate_id AND e.payload_json::jsonb->'version'='1'::jsonb
AND (SELECT count(*) FROM outbox_events all_events WHERE all_events.command_id=e.command_id)=1
AND EXISTS(SELECT 1 FROM command_receipts r JOIN finance_parties p ON p.id=r.record_id
 WHERE r.command_id=e.command_id AND r.operation=e.event_type AND r.record_id=e.aggregate_id
 AND p.kind='customer' AND p.company_id=e.payload_json::jsonb->>'companyId'
 AND r.authority_json::jsonb->>'companyId'=p.company_id
 AND r.result_json::jsonb->>'id'=p.id AND r.result_json::jsonb->>'kind'='customer'
 AND r.result_json::jsonb->'_serviceCustomerPreparation'=jsonb_build_object('kind','native-service-customer.v1','companyId',p.company_id,'commandId',r.command_id)
 AND (SELECT count(*) FROM audit_events a WHERE a.command_id=r.command_id)=1
 AND EXISTS(SELECT 1 FROM audit_events a WHERE a.command_id=r.command_id AND a.actor_id=r.actor_id
  AND a.action=e.event_type AND a.record_type=e.aggregate_type AND a.record_id=e.aggregate_id AND a.record_version=1
  AND a.authority_json=r.authority_json AND a.created_at=r.created_at AND e.created_at=r.created_at)))
  ),false) THEN RETURN true; END IF;
 END IF;
 IF e.event_type='finance.invoice.post' THEN
  IF coalesce((
(
e.event_type='finance.invoice.post' AND e.aggregate_type='finance_invoice'
AND e.aggregate_version BETWEEN 2 AND 9007199254740991
AND e.payload_json::jsonb->>'id'=e.aggregate_id
AND e.payload_json::jsonb->'version'=to_jsonb(e.aggregate_version)
AND e.payload_json::jsonb->>'state'='posted'
AND (SELECT count(*) FROM outbox_events all_events WHERE all_events.command_id=e.command_id)=1
AND EXISTS(SELECT 1 FROM command_receipts r JOIN finance_invoices i ON i.id=r.record_id
 JOIN finance_journals j ON j.id=i.journal_id AND j.company_id=i.company_id
 WHERE r.command_id=e.command_id AND r.operation=e.event_type AND r.record_id=e.aggregate_id
 AND i.source_type='field_service' AND i.kind='receivable' AND i.version>=e.aggregate_version
 AND j.source_type='invoice' AND j.source_id=i.id
 AND i.company_id=e.payload_json::jsonb->>'companyId'
 AND r.authority_json::jsonb->>'companyId'=i.company_id
 AND r.result_json::jsonb->>'id'=i.id AND r.result_json::jsonb->>'status'='posted'
 AND r.result_json::jsonb->>'journalId'=j.id
 AND (SELECT count(*) FROM audit_events a WHERE a.command_id=r.command_id)=1
 AND EXISTS(SELECT 1 FROM audit_events a WHERE a.command_id=r.command_id AND a.actor_id=r.actor_id
  AND a.action=e.event_type AND a.record_type=e.aggregate_type AND a.record_id=e.aggregate_id
  AND a.record_version=e.aggregate_version AND a.authority_json=r.authority_json
  AND a.created_at=r.created_at AND e.created_at=r.created_at))
)
  ),false) THEN RETURN true; END IF;
 END IF;
 IF e.event_type IN('finance.payment.create','finance.payment.approve','finance.payment.release','finance.payment.confirm','finance.bank.import') THEN
  IF coalesce((
(
e.aggregate_version BETWEEN 1 AND 9007199254740991
AND e.payload_json::jsonb->>'id'=e.aggregate_id
AND e.payload_json::jsonb->'version'=to_jsonb(e.aggregate_version)
AND (SELECT count(*) FROM outbox_events all_events WHERE all_events.command_id=e.command_id)=1
AND EXISTS(SELECT 1 FROM command_receipts r
 WHERE r.command_id=e.command_id AND r.operation=e.event_type AND r.record_id=e.aggregate_id
 AND r.result_json::jsonb->>'id'=e.aggregate_id
 AND r.authority_json::jsonb->>'companyId'=e.payload_json::jsonb->>'companyId'
 AND e.payload_json::jsonb->>'state'=r.result_json::jsonb->>'status'
 AND (
  (e.aggregate_type='finance_payment'
   AND e.event_type IN('finance.payment.create','finance.payment.approve','finance.payment.release','finance.payment.confirm')
   AND erpx_finance_field_payment_owned(e.aggregate_id)
   AND r.result_json::jsonb->'version'=to_jsonb(e.aggregate_version)
   AND EXISTS(SELECT 1 FROM finance_payments p WHERE p.id=e.aggregate_id AND p.kind='receipt'
    AND p.company_id=e.payload_json::jsonb->>'companyId' AND p.version>=e.aggregate_version))
  OR (e.aggregate_type='finance_bank' AND e.event_type='finance.bank.import' AND e.aggregate_version=1
   AND r.result_json::jsonb->'imported'='1'::jsonb AND r.result_json::jsonb->>'status'='imported'
   AND erpx_finance_field_statement_owned(e.aggregate_id)
   AND erpx_finance_field_payment_owned(r.result_json::jsonb->>'settlementPaymentId')
   AND EXISTS(SELECT 1 FROM finance_bank_statements b WHERE b.id=e.aggregate_id
    AND b.company_id=e.payload_json::jsonb->>'companyId')))
 AND (SELECT count(*) FROM audit_events a WHERE a.command_id=r.command_id)=1
 AND EXISTS(SELECT 1 FROM audit_events a WHERE a.command_id=r.command_id AND a.actor_id=r.actor_id
  AND a.action=e.event_type AND a.record_type=e.aggregate_type AND a.record_id=e.aggregate_id
  AND a.record_version=e.aggregate_version AND a.authority_json=r.authority_json
  AND a.created_at=r.created_at AND e.created_at=r.created_at))
)
  ),false) THEN RETURN true; END IF;
 END IF;
 IF e.event_type='flow.assign' THEN
  IF coalesce((
(
 e.event_type='flow.assign' AND e.aggregate_type='flow_assign'
 AND e.aggregate_version BETWEEN 1 AND 9007199254740991
 AND e.payload_json::jsonb->>'id'=e.aggregate_id
 AND e.payload_json::jsonb->'version'=to_jsonb(e.aggregate_version)
 AND (SELECT count(*) FROM outbox_events all_events WHERE all_events.command_id=e.command_id)=1
 AND EXISTS(SELECT 1 FROM command_receipts r
  JOIN finance_invoices i ON i.id=r.record_id AND i.company_id=r.authority_json::jsonb->>'companyId'
  JOIN flow_events f ON f.command_id=r.command_id AND f.company_id=i.company_id AND f.kind='invoice' AND f.record_id=i.id
  WHERE r.command_id=e.command_id AND r.operation='flow.assign' AND r.record_id=e.aggregate_id
  AND i.source_type='field_service' AND i.version>=e.aggregate_version
  AND i.company_id=e.payload_json::jsonb->>'companyId'
  AND r.result_json::jsonb->>'id'=i.id
  AND CASE WHEN jsonb_typeof(r.result_json::jsonb->'version')='number' THEN (r.result_json::jsonb->>'version')::numeric END BETWEEN 1 AND 9007199254740991
  AND f.action='assign' AND f.actor_user_id=r.actor_id AND f.record_version=e.aggregate_version
  AND f.created_at BETWEEN 0 AND 9007199254740991
  AND (SELECT count(*) FROM flow_events all_events WHERE all_events.command_id=r.command_id)=1
  AND (SELECT count(*) FROM audit_events a WHERE a.command_id=r.command_id)=1
  AND EXISTS(SELECT 1 FROM audit_events a WHERE a.command_id=r.command_id AND a.actor_id=r.actor_id
   AND a.action='flow.assign' AND a.record_type='flow_assign' AND a.record_id=i.id AND a.record_version=e.aggregate_version
   AND a.authority_json=r.authority_json AND a.created_at=r.created_at AND e.created_at=r.created_at))
 )
  ),false) THEN RETURN true; END IF;
 END IF;
 IF e.event_type IN('finance.chart.create','finance.account.create','finance.period.create','money.rate.confirm') THEN
  IF coalesce((
(
 e.event_type IN('finance.chart.create','finance.account.create','finance.period.create','money.rate.confirm')
 AND e.aggregate_type=CASE e.event_type WHEN 'finance.chart.create' THEN 'finance_chart' WHEN 'finance.account.create' THEN 'finance_account' WHEN 'finance.period.create' THEN 'finance_period' ELSE 'money_rate' END
 AND e.aggregate_version=CASE WHEN e.event_type='finance.period.create' THEN 0 ELSE 1 END
 AND e.payload_json::jsonb->>'id'=e.aggregate_id
 AND e.payload_json::jsonb->'version'=to_jsonb(e.aggregate_version)
 AND (SELECT count(*) FROM outbox_events all_events WHERE all_events.command_id=e.command_id)=1
 AND EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=e.command_id
  AND r.operation=e.event_type AND r.record_id=e.aggregate_id
  AND r.authority_json::jsonb->>'companyId'=e.payload_json::jsonb->>'companyId'
  AND r.result_json::jsonb->>'id'=e.aggregate_id
  AND r.result_json::jsonb->'_servicePartsPrerequisite'=jsonb_build_object(
   'kind','native-service-prerequisite.v1','companyId',e.payload_json::jsonb->>'companyId',
   'commandId',r.command_id,'operation',r.operation,'recordId',r.record_id)
  AND (
   (e.event_type='finance.chart.create' AND r.result_json::jsonb->'version'='1'::jsonb
    AND EXISTS(SELECT 1 FROM finance_charts c WHERE c.id=e.aggregate_id AND c.company_id=e.payload_json::jsonb->>'companyId'))
   OR (e.event_type='finance.account.create' AND r.result_json::jsonb->'version'='1'::jsonb
    AND EXISTS(SELECT 1 FROM finance_accounts a WHERE a.id=e.aggregate_id AND a.company_id=e.payload_json::jsonb->>'companyId'))
   OR (e.event_type='finance.period.create' AND r.result_json::jsonb->'version'='0'::jsonb AND r.result_json::jsonb->>'state'='open'
    AND EXISTS(SELECT 1 FROM finance_periods p WHERE p.id=e.aggregate_id AND p.company_id=e.payload_json::jsonb->>'companyId'))
   OR (e.event_type='money.rate.confirm' AND EXISTS(SELECT 1 FROM company_fx_rates f WHERE f.id=e.aggregate_id
    AND f.company_id=e.payload_json::jsonb->>'companyId' AND f.confirmed_by=r.actor_id
    AND f.currency=r.result_json::jsonb->>'currency' AND f.effective_date::text=r.result_json::jsonb->>'effectiveDate'
    AND to_jsonb(f.confirmed_at)=r.result_json::jsonb->'confirmedAt')))
  AND (SELECT count(*) FROM audit_events a WHERE a.command_id=r.command_id)=1
  AND EXISTS(SELECT 1 FROM audit_events a WHERE a.command_id=r.command_id AND a.actor_id=r.actor_id
   AND a.action=e.event_type AND a.record_type=e.aggregate_type AND a.record_id=e.aggregate_id AND a.record_version=e.aggregate_version
   AND a.authority_json=r.authority_json AND a.created_at=r.created_at AND e.created_at=r.created_at))
 )
  ),false) THEN RETURN true; END IF;
 END IF;
 IF e.event_type='finance.service-no-charge.confirm' THEN
  IF coalesce((
erpx_native_service_no_charge_outbox_eligible(e)
  ),false) THEN RETURN true; END IF;
 END IF;
 IF e.event_type IN('flow.assign','finance.journal.post') THEN
  IF coalesce((
erpx_native_field_journal_outbox_eligible(e)
  ),false) THEN RETURN true; END IF;
 END IF;
 RETURN false;
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
((((e.event_type LIKE 'collaboration.action.%' AND EXISTS(SELECT 1 FROM collaboration_action_cards s WHERE s.id=e.aggregate_id AND s.source_type='field_service_order')) OR (e.event_type LIKE 'collaboration.relation.%' AND EXISTS(SELECT 1 FROM collaboration_record_relations s WHERE s.id=e.aggregate_id AND ((s.source_type='field_service_order' AND s.target_type IN ('field_service_order','support_ticket')) OR (s.source_type='support_ticket' AND s.target_type='field_service_order'))))) OR (e.event_type='inbox.delivery.state' AND (EXISTS(SELECT 1 FROM inbox_deliveries d JOIN inbox_events n ON n.id=d.event_id AND n.company_id=d.company_id WHERE d.id=e.aggregate_id AND n.source_type IN('field_service_order','operational_exception','collaboration_action','support_ticket','stock_count','invoice','journal') AND erpx_field_push_source(n.source_type,n.source_id,n.company_id)) OR EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=e.command_id AND (CASE WHEN r.result_json IS JSON OBJECT THEN r.result_json::jsonb ELSE '{}'::jsonb END)->'_nativeFieldInbox'='true'::jsonb))))) OR
(e.event_type IN ('attachment.set.create','attachment.record.bind')
AND EXISTS(SELECT 1 FROM attachment_sets s WHERE erpx_files_native_target_owned(s.target_type,s.target_id)
 AND (s.bound_command_id=e.command_id
  OR EXISTS(SELECT 1 FROM attachment_events origin WHERE origin.set_id=s.id AND origin.action='set.native-origin' AND origin.details_json::jsonb->>'commandId'=e.command_id)
  OR EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=e.command_id AND (CASE WHEN r.result_json IS JSON OBJECT THEN r.result_json::jsonb ELSE '{}'::jsonb END)->>'setId'=s.id)))) OR
((((e.event_type='finance.party.create' AND EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=e.command_id AND jsonb_exists(CASE WHEN r.result_json IS JSON OBJECT THEN r.result_json::jsonb ELSE '{}'::jsonb END,'_serviceCustomerPreparation'))) OR (e.event_type='finance.invoice.post' AND EXISTS(SELECT 1 FROM finance_invoices i WHERE i.id=e.aggregate_id AND i.source_type='field_service'))) OR ((e.aggregate_type='finance_payment' AND e.event_type IN('finance.payment.create','finance.payment.approve','finance.payment.release','finance.payment.confirm') AND erpx_finance_field_payment_owned(e.aggregate_id)) OR (e.aggregate_type='finance_bank' AND e.event_type='finance.bank.import' AND erpx_finance_field_statement_owned(e.aggregate_id)))) OR (e.aggregate_type='flow_assign' AND e.event_type='flow.assign' AND erpx_finance_field_invoice_owned(e.aggregate_id,NULL))
 OR (EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=e.command_id AND jsonb_exists(CASE WHEN r.result_json IS JSON OBJECT THEN r.result_json::jsonb ELSE '{}'::jsonb END,'_servicePartsPrerequisite'))) OR (e.event_type='finance.service-no-charge.confirm' AND erpx_finance_field_invoice_owned(e.aggregate_id,NULL)) OR (EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=e.command_id AND r.operation='finance.service-no-charge.confirm' AND erpx_finance_field_invoice_owned(r.record_id,r.authority_json::jsonb->>'companyId'))) OR (
 (e.event_type='finance.journal.post' AND erpx_finance_field_journal_owned(e.aggregate_id,NULL))
 OR EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=e.command_id AND r.operation='finance.journal.post'
  AND erpx_finance_field_journal_owned(r.record_id,r.authority_json::jsonb->>'companyId'))
 OR (
  ((e.event_type='flow.assign' AND e.aggregate_type='flow_assign' AND erpx_finance_field_journal_owned(e.aggregate_id,NULL))
   OR EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=e.command_id AND r.operation='flow.assign'
    AND erpx_finance_field_journal_owned(r.record_id,r.authority_json::jsonb->>'companyId')))
  AND NOT (
   (SELECT count(*) FROM flow_events all_events WHERE all_events.command_id=e.command_id)=1
   AND EXISTS(SELECT 1 FROM flow_events f JOIN command_receipts r ON r.command_id=f.command_id
    WHERE f.command_id=e.command_id AND f.kind IN('expense','leave','staff_task','request','stock_count','transfer','order','invoice','payroll','record')
     AND f.action='assign' AND f.record_id=e.aggregate_id AND f.record_version=e.aggregate_version
     AND f.company_id=e.payload_json::jsonb->>'companyId' AND f.actor_user_id=r.actor_id
     AND r.operation='flow.assign' AND r.record_id=f.record_id AND r.authority_json::jsonb->>'companyId'=f.company_id
     AND e.payload_json::jsonb->>'id'=e.aggregate_id AND e.payload_json::jsonb->'version'=to_jsonb(e.aggregate_version))
  )
 )
))
),false) FROM (SELECT (candidate).*) e
$$;
