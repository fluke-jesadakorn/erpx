ALTER TABLE decision_evidence ADD COLUMN authority_json TEXT;
ALTER TABLE social_ai_drafts ADD COLUMN actor_json TEXT;
CREATE FUNCTION capture_decision_work_authority() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.authority_json IS NULL THEN
    SELECT authority_json INTO NEW.authority_json FROM command_receipts WHERE command_id=NEW.command_id;
  END IF;
  RETURN NEW;
END;
$$;
CREATE TRIGGER decision_work_authority BEFORE INSERT ON decision_evidence FOR EACH ROW EXECUTE FUNCTION capture_decision_work_authority();
