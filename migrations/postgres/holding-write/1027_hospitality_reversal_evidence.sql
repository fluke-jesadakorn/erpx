-- Frozen request facts and reviewed decisions remain durable evidence.
CREATE FUNCTION pos_room_charge_request_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' THEN RAISE EXCEPTION 'POS_REVERSAL_EVIDENCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF (to_jsonb(NEW)-ARRAY['state','version','pos_approved_by','pms_approved_by','rejected_by','review_reason','updated_at']) IS DISTINCT FROM
    (to_jsonb(OLD)-ARRAY['state','version','pos_approved_by','pms_approved_by','rejected_by','review_reason','updated_at'])
    OR NEW.version<>OLD.version+1 OR length(trim(COALESCE(NEW.review_reason,'')))<3
    OR NOT ((OLD.state='requested' AND NEW.state IN ('pos_approved','rejected')) OR (OLD.state='pos_approved' AND NEW.state IN ('approved','rejected')))
    OR (OLD.pos_approved_by IS NOT NULL AND NEW.pos_approved_by IS DISTINCT FROM OLD.pos_approved_by)
 THEN RAISE EXCEPTION 'POS_REVERSAL_EVIDENCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF NEW.state='pos_approved' AND (NEW.pos_approved_by IS NULL OR iam_same_person(NEW.pos_approved_by,NEW.requested_by,NEW.company_id))
 OR NEW.state='approved' AND (NEW.pms_approved_by IS NULL OR iam_same_person(NEW.pms_approved_by,NEW.requested_by,NEW.company_id) OR iam_same_person(NEW.pms_approved_by,NEW.pos_approved_by,NEW.company_id))
 OR NEW.state='rejected' AND NEW.rejected_by IS NULL
 THEN RAISE EXCEPTION 'POS_INDEPENDENT_REVIEW' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER pos_room_charge_request_guard BEFORE UPDATE OR DELETE ON pos_room_charge_reversal_requests FOR EACH ROW EXECUTE FUNCTION pos_room_charge_request_guard();

CREATE FUNCTION pos_linked_property_scope_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.property_id IS NOT NULL AND (NEW.mode<>'restaurant' OR NOT EXISTS(SELECT 1 FROM pms_properties p WHERE p.id=NEW.property_id AND p.company_id=NEW.company_id AND p.branch_id=NEW.branch_id))
 THEN RAISE EXCEPTION 'POS_PROPERTY_SCOPE' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER pos_linked_property_scope_guard BEFORE INSERT OR UPDATE ON pos_registers FOR EACH ROW EXECUTE FUNCTION pos_linked_property_scope_guard();
