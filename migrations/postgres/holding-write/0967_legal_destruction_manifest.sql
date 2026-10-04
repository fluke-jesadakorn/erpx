-- Preserve the exact independently reviewed destruction request across retry.
-- New manifest fields describe existing canonical File Sources; old requests
-- requiring a wider set of objects remain blocked for a new owner review.
CREATE FUNCTION legal_destruction_evidence_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' THEN RAISE EXCEPTION 'LEGAL_DESTRUCTION_EVIDENCE_IMMUTABLE'; END IF;
 IF NEW.id IS DISTINCT FROM OLD.id OR NEW.item_id IS DISTINCT FROM OLD.item_id
  OR NEW.policy_id IS DISTINCT FROM OLD.policy_id OR NEW.policy_version IS DISTINCT FROM OLD.policy_version
  OR NEW.requested_by IS DISTINCT FROM OLD.requested_by OR NEW.reason IS DISTINCT FROM OLD.reason
  OR NEW.manifest IS DISTINCT FROM OLD.manifest OR NEW.created_at IS DISTINCT FROM OLD.created_at THEN
  RAISE EXCEPTION 'LEGAL_DESTRUCTION_EVIDENCE_IMMUTABLE';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER legal_destruction_evidence_immutable BEFORE UPDATE OR DELETE ON legal_destructions
 FOR EACH ROW EXECUTE FUNCTION legal_destruction_evidence_immutable();
