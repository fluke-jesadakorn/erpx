-- Direct Service intake retains the Support case owner and existing order identity.
-- No package grant, IAM assignment, rollout or price is changed.
ALTER TABLE field_service_agreements ADD CONSTRAINT service_agreement_company_identity UNIQUE(company_id,id);
ALTER TABLE field_service_orders ADD CONSTRAINT service_order_company_identity UNIQUE(company_id,id);
ALTER TABLE support_tickets
 ADD COLUMN service_agreement_id text,
 ADD COLUMN service_order_id text UNIQUE,
 ADD COLUMN service_order_version bigint,
 ADD CONSTRAINT support_service_agreement FOREIGN KEY(company_id,service_agreement_id) REFERENCES field_service_agreements(company_id,id),
 ADD CONSTRAINT support_service_order FOREIGN KEY(company_id,service_order_id) REFERENCES field_service_orders(company_id,id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE support_tickets DROP CONSTRAINT support_ticket_source_kind;
ALTER TABLE support_tickets ADD CONSTRAINT support_ticket_source_kind CHECK(
 (service_agreement_id IS NULL AND service_order_id IS NULL AND service_order_version IS NULL AND (
   source_kind='sales_order' AND branch_id IS NOT NULL AND order_id IS NOT NULL AND customer_id IS NOT NULL AND source_record_json IS NULL
   OR source_kind='general' AND order_id IS NULL AND customer_id IS NULL AND source_record_json IS NULL
   OR source_kind IN ('pos_sale','pms_record','asset') AND order_id IS NULL AND customer_id IS NULL AND source_record_json IS NOT NULL
      AND source_record_json::jsonb->>'kind'=source_kind AND length(source_record_json::jsonb->>'id')>0
      AND (source_record_json::jsonb->>'version')::bigint>0))
 OR source_kind='field_service' AND branch_id IS NOT NULL AND order_id IS NULL AND customer_id IS NOT NULL
   AND service_agreement_id IS NOT NULL AND service_order_id IS NOT NULL AND service_order_version IS NOT NULL AND service_order_version>0 AND source_record_json IS NULL AND sla_clock_json IS NULL
   AND status IN ('open','closed','cancelled'));
CREATE OR REPLACE FUNCTION support_ticket_source_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF (NEW.company_id,NEW.branch_id,NEW.department_id,NEW.order_id,NEW.customer_id,NEW.source_kind,NEW.source_record_json,NEW.created_by,NEW.service_agreement_id,NEW.service_order_id)
  IS DISTINCT FROM (OLD.company_id,OLD.branch_id,OLD.department_id,OLD.order_id,OLD.customer_id,OLD.source_kind,OLD.source_record_json,OLD.created_by,OLD.service_agreement_id,OLD.service_order_id)
 THEN RAISE EXCEPTION 'SUPPORT_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE FUNCTION support_service_intake_link_required() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF NEW.source_kind='field_service' AND NOT EXISTS(
  SELECT 1 FROM field_service_orders o JOIN field_service_agreements g ON g.id=o.agreement_id AND g.company_id=o.company_id
  WHERE o.id=NEW.service_order_id AND o.company_id=NEW.company_id AND o.ticket_id=NEW.id
    AND o.agreement_id=NEW.service_agreement_id AND g.customer_id=NEW.customer_id
    AND o.branch_id=NEW.branch_id AND o.department_id IS NOT DISTINCT FROM NEW.department_id) THEN
  RAISE EXCEPTION 'SERVICE_INTAKE_LINK_REQUIRED' USING ERRCODE='23514';
 END IF; RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER support_service_intake_link AFTER INSERT OR UPDATE ON support_tickets
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION support_service_intake_link_required();
CREATE FUNCTION service_order_intake_source_required() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF EXISTS(SELECT 1 FROM support_tickets t WHERE t.id=NEW.ticket_id AND t.source_kind='field_service'
  AND (t.service_order_id<>NEW.id OR t.service_agreement_id<>NEW.agreement_id OR t.company_id<>NEW.company_id
    OR t.branch_id<>NEW.branch_id OR t.department_id IS DISTINCT FROM NEW.department_id)) THEN
  RAISE EXCEPTION 'SERVICE_INTAKE_LINK_REQUIRED' USING ERRCODE='23514';
 END IF; RETURN NEW;
END $$;
CREATE TRIGGER service_order_intake_source BEFORE INSERT ON field_service_orders FOR EACH ROW EXECUTE FUNCTION service_order_intake_source_required();

-- Service order events remain authoritative. The projection also supports the
-- unchanged pre-intake Service writer while retaining immutable Support history.
CREATE FUNCTION support_service_case_update_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE event_id text:=current_setting('erpx.service_case_projection',true);
BEGIN
 IF OLD.source_kind='field_service' AND (event_id IS NULL OR event_id='' OR NEW.version<>OLD.version+1
   OR NEW.service_order_version<=OLD.service_order_version OR NOT EXISTS(
     SELECT 1 FROM field_service_order_events e WHERE e.id=event_id AND e.order_id=NEW.service_order_id AND e.version=NEW.service_order_version)) THEN
  RAISE EXCEPTION 'SERVICE_CASE_MANAGED_BY_ORDER' USING ERRCODE='23514';
 END IF; RETURN NEW;
END $$;
CREATE TRIGGER support_ticket_source_managed_update BEFORE UPDATE ON support_tickets FOR EACH ROW EXECUTE FUNCTION support_service_case_update_guard();
CREATE FUNCTION support_project_service_order_case() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE o field_service_orders%ROWTYPE; t support_tickets%ROWTYPE; e field_service_order_events%ROWTYPE;
 next_status text; target_due_date text; prior_marker text:=current_setting('erpx.service_case_projection',true);
 ticket_version bigint; completed_by text;
BEGIN
 SELECT * INTO o FROM field_service_orders WHERE id=NEW.id;
 SELECT * INTO t FROM support_tickets WHERE id=o.ticket_id;
 IF t.source_kind IS DISTINCT FROM 'field_service' THEN RETURN NEW; END IF;
 SELECT * INTO e FROM field_service_order_events WHERE order_id=o.id AND version=o.version;
 IF e.id IS NULL OR e.status<>o.status OR e.technician_id<>o.technician_id OR e.scheduled_at IS DISTINCT FROM o.scheduled_at
   OR t.service_order_id<>o.id OR t.service_agreement_id<>o.agreement_id OR t.company_id<>o.company_id
   OR t.branch_id<>o.branch_id OR t.department_id IS DISTINCT FROM o.department_id OR t.issue<>o.issue THEN
  RAISE EXCEPTION 'SERVICE_INTAKE_EVENT_REQUIRED' USING ERRCODE='23514';
 END IF;
 SELECT actor_id INTO completed_by FROM field_service_order_events WHERE order_id=o.id AND action='complete' ORDER BY version DESC LIMIT 1;
 IF o.status='billed' AND (e.action<>'release-billing' OR iam_same_person(e.actor_id,o.technician_id,o.company_id) OR completed_by IS NULL OR iam_same_person(e.actor_id,completed_by,o.company_id) OR NOT EXISTS(
   SELECT 1 FROM field_service_billing_releases r WHERE r.order_id=o.id AND r.source_order_version=o.version AND r.released_by=e.actor_id)) THEN
  RAISE EXCEPTION 'SERVICE_INTAKE_BILLING_REVIEW_REQUIRED' USING ERRCODE='23514';
 END IF;
 IF o.status='cancelled' AND e.action<>'cancel' THEN RAISE EXCEPTION 'SERVICE_INTAKE_EVENT_REQUIRED' USING ERRCODE='23514'; END IF;
 next_status:=CASE o.status WHEN 'billed' THEN 'closed' WHEN 'cancelled' THEN 'cancelled' ELSE 'open' END;
 target_due_date:=to_char(to_timestamp(coalesce(o.scheduled_at,o.updated_at)/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD');
 IF t.service_order_version=o.version THEN
  IF t.status<>next_status OR t.owner_id<>o.technician_id OR t.due_date<>target_due_date THEN
   RAISE EXCEPTION 'SERVICE_INTAKE_PROJECTION_REQUIRED' USING ERRCODE='23514';
  END IF; RETURN NEW;
 END IF;
 IF t.service_order_version>o.version THEN RAISE EXCEPTION 'SERVICE_INTAKE_PROJECTION_REQUIRED' USING ERRCODE='23514'; END IF;
 PERFORM set_config('erpx.service_case_projection',e.id,true);
 UPDATE support_tickets SET status=next_status,owner_id=o.technician_id,due_date=target_due_date,
  resolution=CASE WHEN next_status='closed' THEN o.completion_evidence ELSE NULL END,
  resolved_by=CASE WHEN next_status='closed' THEN completed_by ELSE NULL END,
  resolved_at=CASE WHEN next_status='closed' THEN o.visit_completed_at ELSE NULL END,
  confirmed_by=CASE WHEN next_status='closed' THEN e.actor_id ELSE NULL END,
  confirmed_at=CASE WHEN next_status='closed' THEN o.updated_at ELSE NULL END,
  confirmation_reason=CASE WHEN next_status='closed' THEN e.reason ELSE NULL END,
  service_order_version=o.version,version=version+1,updated_at=o.updated_at WHERE id=t.id RETURNING version INTO ticket_version;
 INSERT INTO support_ticket_events(id,ticket_id,version,action,actor_id,reason,owner_id,due_date,status,created_at,evidence_json)
  VALUES('ticket-service-event-'||e.id,t.id,ticket_version,'service-'||e.action,e.actor_id,e.reason,o.technician_id,target_due_date,next_status,o.updated_at,
    jsonb_build_object('serviceOrderId',o.id,'serviceOrderVersion',o.version,'serviceEventId',e.id));
 PERFORM set_config('erpx.service_case_projection',coalesce(prior_marker,''),true);
 RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER support_service_order_projection AFTER INSERT OR UPDATE ON field_service_orders
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION support_project_service_order_case();

-- Keep independent review and outstanding billing safe with the established
-- pre-intake writer as well as the current Service application.
CREATE FUNCTION service_independent_agreement_review_required() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE g field_service_agreements%ROWTYPE; e field_service_agreement_events%ROWTYPE;
BEGIN
 SELECT * INTO g FROM field_service_agreements WHERE id=NEW.id;
 IF g.status='draft' THEN RETURN NEW; END IF;
 SELECT * INTO e FROM field_service_agreement_events WHERE agreement_id=g.id AND version=g.version;
 IF e.id IS NULL OR e.status<>g.status OR e.actor_id IS DISTINCT FROM g.reviewed_by OR iam_same_person(e.actor_id,g.created_by,g.company_id) THEN
  RAISE EXCEPTION 'SERVICE_AGREEMENT_INDEPENDENT_REVIEW_REQUIRED' USING ERRCODE='23514';
 END IF;
 IF g.status='retired' AND EXISTS(SELECT 1 FROM field_service_orders WHERE agreement_id=g.id AND status NOT IN ('cancelled','billed')) THEN
  RAISE EXCEPTION 'SERVICE_AGREEMENT_BILLING_OUTSTANDING' USING ERRCODE='23514';
 END IF;RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER service_independent_agreement_review AFTER INSERT OR UPDATE ON field_service_agreements
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION service_independent_agreement_review_required();
CREATE FUNCTION service_independent_billing_review_required() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE o field_service_orders%ROWTYPE; e field_service_order_events%ROWTYPE; completed_by text;
BEGIN
 SELECT * INTO o FROM field_service_orders WHERE id=NEW.id;
 IF o.status<>'billed' THEN RETURN NEW; END IF;
 SELECT * INTO e FROM field_service_order_events WHERE order_id=o.id AND version=o.version;
 SELECT actor_id INTO completed_by FROM field_service_order_events WHERE order_id=o.id AND action='complete' ORDER BY version DESC LIMIT 1;
 IF e.id IS NULL OR e.action<>'release-billing' OR e.status<>'billed' OR completed_by IS NULL
  OR iam_same_person(e.actor_id,o.technician_id,o.company_id) OR iam_same_person(e.actor_id,completed_by,o.company_id)
  OR NOT EXISTS(SELECT 1 FROM field_service_billing_releases WHERE order_id=o.id AND source_order_version=o.version AND released_by=e.actor_id) THEN
  RAISE EXCEPTION 'SERVICE_BILLING_INDEPENDENT_REVIEW_REQUIRED' USING ERRCODE='23514';
 END IF;RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER service_independent_billing_review AFTER INSERT OR UPDATE ON field_service_orders
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION service_independent_billing_review_required();
