ALTER TABLE finance_asset_disposals ADD COLUMN policy_source_hash TEXT;
CREATE FUNCTION protect_finance_disposal_policy_evidence() RETURNS TRIGGER LANGUAGE plpgsql AS $$ BEGIN
 IF NEW.policy_source_hash IS DISTINCT FROM OLD.policy_source_hash THEN RAISE EXCEPTION 'ASSET_DISPOSAL_POLICY_EVIDENCE_IMMUTABLE' USING ERRCODE='23514';END IF;RETURN NEW;END;$$;
CREATE TRIGGER finance_disposal_policy_evidence_immutable BEFORE UPDATE ON finance_asset_disposals FOR EACH ROW EXECUTE FUNCTION protect_finance_disposal_policy_evidence();
