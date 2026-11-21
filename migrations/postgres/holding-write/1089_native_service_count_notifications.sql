-- Native Service first Count Inbox/Staff state shares the existing owner partition.
-- Other Inventory Count capabilities retain their declared runtime ownership.
-- Native acknowledgement ownership. This does not remove deliveries/events.
CREATE OR REPLACE FUNCTION erpx_require_native_field_inbox_state() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF current_setting('erpx.inbox_state_runtime',true) IS DISTINCT FROM 'native-rust'
 AND (NEW.read_at IS DISTINCT FROM OLD.read_at OR NEW.archived_at IS DISTINCT FROM OLD.archived_at)
 AND EXISTS(SELECT 1 FROM inbox_events e WHERE e.source_type IN('field_service_order','operational_exception','collaboration_action','support_ticket','stock_count') AND erpx_field_push_source(e.source_type,e.source_id,e.company_id) AND ((e.id=OLD.event_id AND e.company_id=OLD.company_id) OR (e.id=NEW.event_id AND e.company_id=NEW.company_id))) THEN
  RAISE EXCEPTION 'Field Inbox state requires the native runtime' USING ERRCODE='23514',CONSTRAINT='native_field_inbox_state_required';
 END IF;
 RETURN NEW;
END $$;
CREATE OR REPLACE FUNCTION erpx_require_native_field_staff_state() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF current_setting('erpx.inbox_state_runtime',true) IS DISTINCT FROM 'native-rust' AND NEW.read_at IS DISTINCT FROM OLD.read_at AND ((OLD.record_type IN('field_service_order','operational_exception','collaboration_action','support_ticket','stock_count') AND erpx_field_push_source(OLD.record_type,OLD.record_id,OLD.company_id)) OR (NEW.record_type IN('field_service_order','operational_exception','collaboration_action','support_ticket','stock_count') AND erpx_field_push_source(NEW.record_type,NEW.record_id,NEW.company_id))) THEN
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
((((e.event_type LIKE 'collaboration.action.%' AND EXISTS(SELECT 1 FROM collaboration_action_cards s WHERE s.id=e.aggregate_id AND s.source_type='field_service_order')) OR (e.event_type LIKE 'collaboration.relation.%' AND EXISTS(SELECT 1 FROM collaboration_record_relations s WHERE s.id=e.aggregate_id AND ((s.source_type='field_service_order' AND s.target_type IN ('field_service_order','support_ticket')) OR (s.source_type='support_ticket' AND s.target_type='field_service_order'))))) OR (e.event_type='inbox.delivery.state' AND (EXISTS(SELECT 1 FROM inbox_deliveries d JOIN inbox_events n ON n.id=d.event_id AND n.company_id=d.company_id WHERE d.id=e.aggregate_id AND n.source_type IN('field_service_order','operational_exception','collaboration_action','support_ticket','stock_count') AND erpx_field_push_source(n.source_type,n.source_id,n.company_id)) OR EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=e.command_id AND (CASE WHEN r.result_json IS JSON OBJECT THEN r.result_json::jsonb ELSE '{}'::jsonb END)->'_nativeFieldInbox'='true'::jsonb))))) OR
(e.event_type IN ('attachment.set.create','attachment.record.bind')
AND EXISTS(SELECT 1 FROM attachment_sets s WHERE s.target_type='field_service_visit'
 AND (s.bound_command_id=e.command_id
  OR EXISTS(SELECT 1 FROM attachment_events origin WHERE origin.set_id=s.id AND origin.action='set.native-origin' AND origin.details_json::jsonb->>'commandId'=e.command_id)
  OR EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=e.command_id AND (CASE WHEN r.result_json IS JSON OBJECT THEN r.result_json::jsonb ELSE '{}'::jsonb END)->>'setId'=s.id))))
 OR
(e.event_type='finance.party.create' AND EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=e.command_id AND jsonb_exists(CASE WHEN r.result_json IS JSON OBJECT THEN r.result_json::jsonb ELSE '{}'::jsonb END,'_serviceCustomerPreparation')))
),false) FROM (SELECT (candidate).*) e
$$;
