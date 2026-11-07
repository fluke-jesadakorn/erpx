-- Field zero-demand confirmation has one native writer and consumer.
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
((((e.event_type='finance.party.create' AND EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=e.command_id AND jsonb_exists(CASE WHEN r.result_json IS JSON OBJECT THEN r.result_json::jsonb ELSE '{}'::jsonb END,'_serviceCustomerPreparation'))) OR (e.event_type='finance.invoice.post' AND EXISTS(SELECT 1 FROM finance_invoices i WHERE i.id=e.aggregate_id AND i.source_type='field_service'))) OR ((e.aggregate_type='finance_payment' AND e.event_type IN('finance.payment.create','finance.payment.approve','finance.payment.release','finance.payment.confirm') AND erpx_finance_field_payment_owned(e.aggregate_id)) OR (e.aggregate_type='finance_bank' AND e.event_type='finance.bank.import' AND erpx_finance_field_statement_owned(e.aggregate_id)))) OR (e.aggregate_type='flow_assign' AND e.event_type='flow.assign' AND erpx_finance_field_invoice_owned(e.aggregate_id,NULL))
 OR (EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=e.command_id AND jsonb_exists(CASE WHEN r.result_json IS JSON OBJECT THEN r.result_json::jsonb ELSE '{}'::jsonb END,'_servicePartsPrerequisite'))) OR (e.event_type='finance.service-no-charge.confirm' AND erpx_finance_field_invoice_owned(e.aggregate_id,NULL)) OR (EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=e.command_id AND r.operation='finance.service-no-charge.confirm' AND erpx_finance_field_invoice_owned(r.record_id,r.authority_json::jsonb->>'companyId'))))
),false) FROM (SELECT (candidate).*) e
$$;

-- Existing confirmations/data/history remain intact. New Field confirmations
-- must be accompanied by the Finance owner's final native invoice image.
CREATE FUNCTION erpx_guard_native_service_no_charge() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE actual jsonb;
BEGIN
 SELECT to_jsonb(i) INTO actual FROM finance_invoices i WHERE i.id=NEW.invoice_id AND i.company_id=NEW.company_id AND i.source_type='field_service';
 IF actual IS NULL OR erpx_service_receipt_runtime_marker()->'invoices'->NEW.invoice_id IS DISTINCT FROM actual THEN
  RAISE EXCEPTION 'Field no-charge confirmation requires the native owner unit'
   USING ERRCODE='23514',CONSTRAINT='native_service_no_charge_required';
 END IF;
 RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER native_service_no_charge_required AFTER INSERT ON finance_service_no_charge_confirmations
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION erpx_guard_native_service_no_charge();

-- A planning/execution boundary keeps the completed-source proof out of the
-- shared multi-owner lease plan. Every retained proof predicate stays intact.
CREATE FUNCTION erpx_native_service_no_charge_outbox_eligible(candidate outbox_events)
RETURNS boolean LANGUAGE plpgsql STABLE AS $$
DECLARE e outbox_events:=candidate;
BEGIN
 IF e.event_type IS DISTINCT FROM 'finance.service-no-charge.confirm'
 OR e.aggregate_type IS DISTINCT FROM 'finance_service-no-charge'
 OR NOT (e.payload_json IS JSON OBJECT) THEN RETURN false; END IF;
 RETURN coalesce((
e.event_type='finance.service-no-charge.confirm' AND e.aggregate_type='finance_service-no-charge'
AND e.aggregate_version BETWEEN 2 AND 9007199254740991
AND e.payload_json::jsonb->>'id'=e.aggregate_id
AND e.payload_json::jsonb->'version'=to_jsonb(e.aggregate_version)
AND e.payload_json::jsonb->>'state'='no_charge_confirmed'
AND (SELECT count(*) FROM outbox_events events WHERE events.command_id=e.command_id)=1
AND EXISTS(SELECT 1 FROM (
 SELECT * FROM finance_service_no_charge_completed
 WHERE id=e.aggregate_id AND command_id=e.command_id AND company_id=e.payload_json::jsonb->>'companyId'
 OFFSET 0
 ) s JOIN command_receipts r ON r.command_id=s.command_id
 WHERE s.id=e.aggregate_id AND s.command_id=e.command_id
 AND s.company_id=e.payload_json::jsonb->>'companyId' AND s.version=e.aggregate_version
 AND r.operation=e.event_type AND r.record_id=s.id AND r.actor_id=s.confirmed_by
 AND r.authority_json::jsonb->>'companyId'=s.company_id
 AND r.result_json::jsonb->>'id'=s.id AND r.result_json::jsonb->>'confirmationId'=s.confirmation_id
 AND r.result_json::jsonb->>'status'='no_charge_confirmed' AND r.result_json::jsonb->'version'=to_jsonb(s.version)
 AND (SELECT count(*) FROM audit_events a WHERE a.command_id=r.command_id)=1
 AND EXISTS(SELECT 1 FROM audit_events a WHERE a.command_id=r.command_id AND a.actor_id=r.actor_id
  AND a.action=e.event_type AND a.record_type=e.aggregate_type AND a.record_id=s.id
  AND a.record_version=s.version AND a.authority_json=r.authority_json
  AND a.created_at=r.created_at AND e.created_at=r.created_at)
 AND (SELECT count(*) FROM flow_events f WHERE f.command_id=r.command_id)=1
 AND EXISTS(SELECT 1 FROM flow_events f WHERE f.command_id=r.command_id AND f.company_id=s.company_id
  AND f.kind='invoice' AND f.record_id=s.id AND f.actor_user_id=s.confirmed_by
  AND f.step_key='draft' AND f.action='post' AND f.record_version=s.version
  AND f.next_step_key IS NULL AND f.next_user_id IS NULL))
 ),false);
END $$;
