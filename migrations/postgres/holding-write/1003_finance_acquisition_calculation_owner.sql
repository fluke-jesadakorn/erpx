CREATE FUNCTION protect_finance_acquisition_calculation() RETURNS TRIGGER LANGUAGE plpgsql AS $$ BEGIN
 IF OLD.explanation_json::jsonb->>'engine'='source-acquisition-v1' AND (TG_OP='DELETE' OR
 (NEW.id,NEW.company_id,NEW.holding_id,NEW.kind,NEW.policy_id,NEW.policy_version,NEW.period_id,NEW.currency,NEW.currency_scale,NEW.source_hash,NEW.input_json,NEW.result_json,NEW.explanation_json,NEW.prepared_by,NEW.created_at)
 IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.holding_id,OLD.kind,OLD.policy_id,OLD.policy_version,OLD.period_id,OLD.currency,OLD.currency_scale,OLD.source_hash,OLD.input_json,OLD.result_json,OLD.explanation_json,OLD.prepared_by,OLD.created_at)) THEN RAISE EXCEPTION 'ACQUISITION_CALCULATION_IMMUTABLE' USING ERRCODE='23514';END IF;
 RETURN CASE WHEN TG_OP='DELETE' THEN OLD ELSE NEW END; END;$$;
CREATE TRIGGER finance_acquisition_calculation_immutable BEFORE UPDATE OR DELETE ON finance_calculation_runs FOR EACH ROW EXECUTE FUNCTION protect_finance_acquisition_calculation();
