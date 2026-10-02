-- Installed runtime identities predate provisioning defaults in some holdings.
-- Retained facts and events are immutable even when newer provisioning grants
-- generic table mutation privileges.
CREATE FUNCTION protect_finance_ai_proposal() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP='DELETE' OR
    (NEW.company_id,NEW.kind,NEW.source_id,NEW.source_version,NEW.source_hash,
     NEW.source_json,NEW.source_query_json,NEW.company_revision,NEW.policy_version,
     NEW.provider_json,NEW.created_at) IS DISTINCT FROM
    (OLD.company_id,OLD.kind,OLD.source_id,OLD.source_version,OLD.source_hash,
     OLD.source_json,OLD.source_query_json,OLD.company_revision,OLD.policy_version,
     OLD.provider_json,OLD.created_at) OR
    (OLD.result_json IS NOT NULL AND NEW.result_json IS DISTINCT FROM OLD.result_json) THEN
    RAISE EXCEPTION 'FINANCE_AI_EVIDENCE_IMMUTABLE' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END;
$$;
CREATE TRIGGER finance_ai_proposal_evidence BEFORE UPDATE OR DELETE ON finance_ai_proposals
  FOR EACH ROW EXECUTE FUNCTION protect_finance_ai_proposal();
CREATE TRIGGER finance_ai_event_evidence BEFORE UPDATE OR DELETE ON finance_ai_events
  FOR EACH ROW EXECUTE FUNCTION protect_financial_event();

DO $$
DECLARE runtime_role TEXT;
BEGIN
  IF current_user !~ '_migration$' THEN RETURN; END IF;
  FOREACH runtime_role IN ARRAY ARRAY[regexp_replace(current_user,'_migration$','_app'),regexp_replace(current_user,'_migration$','_jobs')] LOOP
    IF NOT EXISTS(SELECT 1 FROM pg_roles WHERE rolname=runtime_role) THEN RAISE EXCEPTION 'ERPX_RUNTIME_ROLES_MISSING'; END IF;
    EXECUTE format('GRANT SELECT,INSERT,UPDATE ON finance_ai_policies,finance_ai_proposals,finance_ai_usage TO %I',runtime_role);
    EXECUTE format('GRANT SELECT,INSERT ON finance_ai_events TO %I',runtime_role);
  END LOOP;
END;
$$;
