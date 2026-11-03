-- Extend the existing native Files partition to evidence of persisted Field
-- receipts. This is routing/quarantine only; native operations retain their
-- current Company, source and transaction checks. Existing rows/history stay.
CREATE FUNCTION erpx_files_native_target_owned(target_kind text,target_ref text)
RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT COALESCE(target_kind='field_service_visit' OR
   (target_kind='finance_payment' AND erpx_finance_field_payment_owned(target_ref)),false)
$$;

CREATE OR REPLACE FUNCTION erpx_files_field_attachment_owned(source_id text)
RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS (
   SELECT 1 FROM attachments a
   LEFT JOIN upload_intents u ON u.id=a.upload_id
   LEFT JOIN attachment_sets s ON s.id=u.attachment_set_id
   WHERE a.id=source_id AND (erpx_files_native_target_owned(a.owner_type,a.owner_id)
     OR erpx_files_native_target_owned(u.owner_type,u.owner_id) OR erpx_files_native_target_owned(s.target_type,s.target_id))
 ) OR EXISTS (
   SELECT 1 FROM attachment_links l
   LEFT JOIN attachment_sets s ON s.id=l.set_id
   LEFT JOIN upload_intents u ON u.id=l.upload_intent_id
   LEFT JOIN attachment_sets us ON us.id=u.attachment_set_id
   WHERE l.attachment_id=source_id AND (erpx_files_native_target_owned(s.target_type,s.target_id)
     OR erpx_files_native_target_owned(u.owner_type,u.owner_id) OR erpx_files_native_target_owned(us.target_type,us.target_id))
 )
$$;

CREATE OR REPLACE FUNCTION erpx_files_guard_field_set_lineage() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE source_id text;
BEGIN
 IF TG_OP='UPDATE' AND ROW(NEW.target_type,NEW.target_id) IS NOT DISTINCT FROM ROW(OLD.target_type,OLD.target_id) THEN RETURN NEW; END IF;
 IF (TG_OP<>'INSERT' AND erpx_files_native_target_owned(OLD.target_type,OLD.target_id)) OR (TG_OP<>'DELETE' AND erpx_files_native_target_owned(NEW.target_type,NEW.target_id)) THEN
   FOR source_id IN SELECT DISTINCT id FROM (
     SELECT l.attachment_id AS id FROM attachment_links l WHERE l.set_id=COALESCE(NEW.id,OLD.id)
     UNION SELECT a.id FROM upload_intents u JOIN attachments a ON a.upload_id=u.id WHERE u.attachment_set_id=COALESCE(NEW.id,OLD.id)
   ) sources WHERE id IS NOT NULL ORDER BY id LOOP PERFORM erpx_files_bump_scan_lineage(source_id); END LOOP;
 END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; END IF; RETURN NEW;
END $$;

CREATE OR REPLACE FUNCTION erpx_files_guard_field_link_lineage() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE old_field boolean := false; new_field boolean := false; source_id text;
BEGIN
 IF TG_OP='UPDATE' AND ROW(NEW.set_id,NEW.attachment_id,NEW.upload_intent_id) IS NOT DISTINCT FROM ROW(OLD.set_id,OLD.attachment_id,OLD.upload_intent_id) THEN RETURN NEW; END IF;
 IF TG_OP<>'INSERT' THEN SELECT EXISTS(SELECT 1 FROM attachment_sets s LEFT JOIN upload_intents u ON u.id=OLD.upload_intent_id LEFT JOIN attachment_sets us ON us.id=u.attachment_set_id WHERE s.id=OLD.set_id AND (erpx_files_native_target_owned(s.target_type,s.target_id) OR erpx_files_native_target_owned(u.owner_type,u.owner_id) OR erpx_files_native_target_owned(us.target_type,us.target_id))) INTO old_field; END IF;
 IF TG_OP<>'DELETE' THEN SELECT EXISTS(SELECT 1 FROM attachment_sets s LEFT JOIN upload_intents u ON u.id=NEW.upload_intent_id LEFT JOIN attachment_sets us ON us.id=u.attachment_set_id WHERE s.id=NEW.set_id AND (erpx_files_native_target_owned(s.target_type,s.target_id) OR erpx_files_native_target_owned(u.owner_type,u.owner_id) OR erpx_files_native_target_owned(us.target_type,us.target_id))) INTO new_field; END IF;
 IF old_field OR new_field THEN
   FOR source_id IN SELECT DISTINCT id FROM (SELECT CASE WHEN TG_OP<>'INSERT' THEN OLD.attachment_id END AS id UNION SELECT CASE WHEN TG_OP<>'DELETE' THEN NEW.attachment_id END) sources WHERE id IS NOT NULL ORDER BY id LOOP PERFORM erpx_files_bump_scan_lineage(source_id); END LOOP;
 END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; END IF; RETURN NEW;
END $$;

CREATE OR REPLACE FUNCTION erpx_files_guard_field_upload_lineage() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE old_field boolean := false; new_field boolean := false; source_id text;
BEGIN
 IF TG_OP='UPDATE' AND ROW(NEW.owner_type,NEW.owner_id,NEW.attachment_set_id) IS NOT DISTINCT FROM ROW(OLD.owner_type,OLD.owner_id,OLD.attachment_set_id) THEN RETURN NEW; END IF;
 IF TG_OP<>'INSERT' THEN SELECT erpx_files_native_target_owned(OLD.owner_type,OLD.owner_id) OR EXISTS(SELECT 1 FROM attachment_sets s WHERE s.id=OLD.attachment_set_id AND erpx_files_native_target_owned(s.target_type,s.target_id)) INTO old_field; END IF;
 IF TG_OP<>'DELETE' THEN SELECT erpx_files_native_target_owned(NEW.owner_type,NEW.owner_id) OR EXISTS(SELECT 1 FROM attachment_sets s WHERE s.id=NEW.attachment_set_id AND erpx_files_native_target_owned(s.target_type,s.target_id)) INTO new_field; END IF;
 IF old_field OR new_field THEN
   FOR source_id IN SELECT DISTINCT id FROM (SELECT a.id FROM attachments a WHERE a.upload_id=COALESCE(NEW.id,OLD.id) UNION SELECT l.attachment_id FROM attachment_links l WHERE l.upload_intent_id=COALESCE(NEW.id,OLD.id)) sources WHERE id IS NOT NULL ORDER BY id LOOP PERFORM erpx_files_bump_scan_lineage(source_id); END LOOP;
 END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; END IF; RETURN NEW;
END $$;

CREATE OR REPLACE FUNCTION erpx_files_legacy_cleanup_set_allowed(input_set_id text) RETURNS boolean LANGUAGE plpgsql VOLATILE AS $$
DECLARE row_source text;
BEGIN
 IF NOT EXISTS(SELECT 1 FROM attachment_sets s WHERE s.id=input_set_id AND NOT erpx_files_native_target_owned(s.target_type,s.target_id)) THEN RETURN false; END IF;
 IF EXISTS(SELECT 1 FROM upload_intents u WHERE u.attachment_set_id=input_set_id AND erpx_files_native_target_owned(u.owner_type,u.owner_id)) THEN RETURN false; END IF;
 IF EXISTS(SELECT 1 FROM attachment_links l JOIN upload_intents u ON u.id=l.upload_intent_id LEFT JOIN attachment_sets s ON s.id=u.attachment_set_id WHERE l.set_id=input_set_id AND (erpx_files_native_target_owned(u.owner_type,u.owner_id) OR erpx_files_native_target_owned(s.target_type,s.target_id))) THEN RETURN false; END IF;
 FOR row_source IN SELECT DISTINCT id FROM (
   SELECT l.attachment_id id FROM attachment_links l WHERE l.set_id=input_set_id
   UNION SELECT a.id FROM upload_intents u JOIN attachments a ON a.upload_id=u.id WHERE u.attachment_set_id=input_set_id
 ) sources WHERE id IS NOT NULL ORDER BY id LOOP
   IF NOT erpx_files_legacy_cleanup_source_allowed(row_source) THEN RETURN false; END IF;
 END LOOP;
 RETURN true;
END $$;

CREATE OR REPLACE FUNCTION erpx_files_assert_legacy_writer(resource_kind text, resource_id text) RETURNS boolean LANGUAGE plpgsql VOLATILE AS $$
DECLARE set_key text; source_key text;
BEGIN
 IF resource_kind='set' THEN set_key:=resource_id;
 ELSIF resource_kind='upload' THEN
   source_key:='attachment-' || resource_id;
   SELECT attachment_set_id INTO set_key FROM upload_intents WHERE id=resource_id FOR UPDATE;
   IF EXISTS(SELECT 1 FROM upload_intents WHERE id=resource_id AND erpx_files_native_target_owned(owner_type,owner_id)) OR EXISTS(SELECT 1 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.upload_intent_id=resource_id AND erpx_files_native_target_owned(s.target_type,s.target_id)) THEN
     RAISE EXCEPTION 'Field file writer is native' USING ERRCODE='23514',CONSTRAINT='field_file_native_writer'; END IF;
 ELSIF resource_kind='link' THEN
   SELECT set_id,attachment_id INTO set_key,source_key FROM attachment_links WHERE id=resource_id FOR UPDATE;
 ELSIF resource_kind='attachment' THEN source_key:=resource_id;
 ELSE RAISE EXCEPTION 'Unknown file writer resource' USING ERRCODE='22023'; END IF;
 IF source_key IS NOT NULL AND NOT erpx_files_legacy_cleanup_source_allowed(source_key) THEN
   RAISE EXCEPTION 'Field file writer is native' USING ERRCODE='23514',CONSTRAINT='field_file_native_writer'; END IF;
 IF set_key IS NOT NULL THEN
   PERFORM 1 FROM attachment_sets WHERE id=set_key FOR UPDATE;
   IF NOT erpx_files_legacy_cleanup_set_allowed(set_key) THEN
     RAISE EXCEPTION 'Field file writer is native' USING ERRCODE='23514',CONSTRAINT='field_file_native_writer'; END IF;
 END IF;
 RETURN true;
END $$;
-- Identity changes that could move an active native source into another
-- partition use the same epoch lock as claim, including owner reference edits.
DROP TRIGGER field_scan_set_lineage ON attachment_sets;
CREATE TRIGGER field_scan_set_lineage AFTER UPDATE OF target_type,target_id OR DELETE ON attachment_sets
 FOR EACH ROW EXECUTE FUNCTION erpx_files_guard_field_set_lineage();
DROP TRIGGER field_scan_upload_lineage ON upload_intents;
CREATE TRIGGER field_scan_upload_lineage AFTER UPDATE OF owner_type,owner_id,attachment_set_id OR DELETE ON upload_intents
 FOR EACH ROW EXECUTE FUNCTION erpx_files_guard_field_upload_lineage();

-- Keep the existing single outbox partition aligned with the owner policies.
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
AND EXISTS(SELECT 1 FROM attachment_sets s WHERE erpx_files_native_target_owned(s.target_type,s.target_id)
 AND (s.bound_command_id=e.command_id
  OR EXISTS(SELECT 1 FROM attachment_events origin WHERE origin.set_id=s.id AND origin.action='set.native-origin' AND origin.details_json::jsonb->>'commandId'=e.command_id)
  OR EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=e.command_id AND (CASE WHEN r.result_json IS JSON OBJECT THEN r.result_json::jsonb ELSE '{}'::jsonb END)->>'setId'=s.id))))
 OR
(((e.event_type='finance.party.create' AND EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=e.command_id AND jsonb_exists(CASE WHEN r.result_json IS JSON OBJECT THEN r.result_json::jsonb ELSE '{}'::jsonb END,'_serviceCustomerPreparation'))) OR (e.event_type='finance.invoice.post' AND EXISTS(SELECT 1 FROM finance_invoices i WHERE i.id=e.aggregate_id AND i.source_type='field_service'))) OR ((e.aggregate_type='finance_payment' AND e.event_type IN('finance.payment.create','finance.payment.approve','finance.payment.release','finance.payment.confirm') AND erpx_finance_field_payment_owned(e.aggregate_id)) OR (e.aggregate_type='finance_bank' AND e.event_type='finance.bank.import' AND erpx_finance_field_statement_owned(e.aggregate_id))))
),false) FROM (SELECT (candidate).*) e
$$;
