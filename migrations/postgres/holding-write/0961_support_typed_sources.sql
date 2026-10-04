-- Typed source references do not confer access to their originating workspace.
ALTER TABLE support_tickets ADD COLUMN source_record_json TEXT CHECK(source_record_json IS NULL OR erpx_json_valid(source_record_json));
ALTER TABLE support_tickets DROP CONSTRAINT support_ticket_source_kind;
ALTER TABLE support_tickets DROP CONSTRAINT support_ticket_status;
ALTER TABLE support_tickets DROP CONSTRAINT support_ticket_closure;
ALTER TABLE support_tickets
 ADD CONSTRAINT support_ticket_source_kind CHECK(
   source_kind='sales_order' AND branch_id IS NOT NULL AND order_id IS NOT NULL AND customer_id IS NOT NULL AND source_record_json IS NULL
   OR source_kind='general' AND order_id IS NULL AND customer_id IS NULL AND source_record_json IS NULL
   OR source_kind IN ('pos_sale','pms_record','asset') AND order_id IS NULL AND customer_id IS NULL AND source_record_json IS NOT NULL
      AND source_record_json::jsonb->>'kind'=source_kind AND length(source_record_json::jsonb->>'id')>0
      AND (source_record_json::jsonb->>'version')::bigint>0),
 ADD CONSTRAINT support_ticket_status CHECK(status IN ('triage','open','in_progress','waiting_customer','resolved','closed','cancelled') AND (status<>'triage' OR source_kind<>'sales_order')),
 ADD CONSTRAINT support_ticket_closure CHECK(status<>'closed' OR resolution IS NOT NULL AND (
   source_kind='sales_order' AND confirmation_note_id IS NOT NULL AND confirmation_note_version IS NOT NULL
   OR source_kind<>'sales_order' AND confirmed_by IS NOT NULL AND confirmed_at IS NOT NULL AND confirmation_reason IS NOT NULL));
CREATE OR REPLACE FUNCTION support_ticket_source_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF (NEW.company_id,NEW.branch_id,NEW.department_id,NEW.order_id,NEW.customer_id,NEW.source_kind,NEW.source_record_json,NEW.created_by)
  IS DISTINCT FROM (OLD.company_id,OLD.branch_id,OLD.department_id,OLD.order_id,OLD.customer_id,OLD.source_kind,OLD.source_record_json,OLD.created_by)
 THEN RAISE EXCEPTION 'SUPPORT_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
-- Support roles were introduced after the common staff permissions were
-- published. Align their persisted role revisions with the common policy.
INSERT INTO iam_role_revisions
 SELECT r.company_id,r.id,r.published_revision+1,v.permissions_json||'["records.read","records.write"]'::jsonb,
 v.grantable_json,v.scopes_json,'system',floor(extract(epoch FROM clock_timestamp())*1000)::bigint
 FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision
 WHERE r.company_id='*' AND r.id IN('support_agent','support_manager');
UPDATE iam_roles SET published_revision=published_revision+1 WHERE company_id='*' AND id IN('support_agent','support_manager');
