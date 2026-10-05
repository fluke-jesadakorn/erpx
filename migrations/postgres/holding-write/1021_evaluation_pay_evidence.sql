ALTER TABLE hr_evaluation_compensation_drafts ADD COLUMN previous_json TEXT NOT NULL DEFAULT '{}' CHECK(erpx_json_valid(previous_json));
ALTER TABLE hr_evaluation_compensation_drafts ADD COLUMN proposed_json TEXT NOT NULL DEFAULT '{}' CHECK(erpx_json_valid(proposed_json));
CREATE FUNCTION hr_evaluation_pay_evidence_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.previous_json<>OLD.previous_json OR NEW.proposed_json<>OLD.proposed_json OR NEW.assessment_id<>OLD.assessment_id OR NEW.assessment_version<>OLD.assessment_version OR NEW.previous_id<>OLD.previous_id OR NEW.previous_version<>OLD.previous_version OR NEW.compensation_id<>OLD.compensation_id OR NEW.policy_id<>OLD.policy_id OR NEW.policy_version<>OLD.policy_version OR NEW.basis_version<>OLD.basis_version OR NEW.created_by<>OLD.created_by THEN
  RAISE EXCEPTION 'EVALUATION_PAY_EVIDENCE_IMMUTABLE';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER hr_evaluation_pay_evidence_immutable BEFORE UPDATE ON hr_evaluation_compensation_drafts FOR EACH ROW EXECUTE FUNCTION hr_evaluation_pay_evidence_immutable();
