ALTER TABLE finance_business_billing_runs ALTER COLUMN invoice_id DROP NOT NULL;
ALTER TABLE finance_business_billing_runs ALTER COLUMN invoice_version DROP NOT NULL;
ALTER TABLE finance_business_billing_runs ADD COLUMN version BIGINT NOT NULL DEFAULT 1;
ALTER TABLE finance_business_billing_runs ADD COLUMN no_charge_status TEXT CHECK(no_charge_status IN ('draft','approved','rejected'));
ALTER TABLE finance_business_billing_runs ADD COLUMN reviewed_by TEXT;
ALTER TABLE finance_business_billing_runs ADD COLUMN review_reason TEXT;
ALTER TABLE finance_business_billing_runs ADD CONSTRAINT finance_billing_no_charge CHECK((invoice_id IS NOT NULL AND invoice_version IS NOT NULL AND no_charge_status IS NULL AND reviewed_by IS NULL) OR (invoice_id IS NULL AND invoice_version IS NULL AND no_charge_status IS NOT NULL AND (result_json::jsonb->>'subtotal')::numeric=0 AND (result_json::jsonb->>'tax')::numeric=0));
ALTER TABLE finance_business_billing_runs ADD CONSTRAINT finance_billing_no_self_review CHECK(reviewed_by IS NULL OR reviewed_by<>created_by);
DROP TRIGGER finance_billing_run_immutable ON finance_business_billing_runs;
CREATE FUNCTION protect_finance_billing_run() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' OR OLD.invoice_id IS NOT NULL OR OLD.no_charge_status IN ('approved','rejected') OR (NEW.id,NEW.company_id,NEW.contract_id,NEW.root_id,NEW.contract_version,NEW.billing_key,NEW.service_from,NEW.service_to,NEW.input_json,NEW.result_json,NEW.source_hash,NEW.invoice_id,NEW.invoice_version,NEW.created_by,NEW.created_at)
 IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.contract_id,OLD.root_id,OLD.contract_version,OLD.billing_key,OLD.service_from,OLD.service_to,OLD.input_json,OLD.result_json,OLD.source_hash,OLD.invoice_id,OLD.invoice_version,OLD.created_by,OLD.created_at) THEN RAISE EXCEPTION 'BILLING_EVIDENCE_IMMUTABLE' USING ERRCODE='23514';END IF;RETURN NEW;END;$$;
CREATE TRIGGER finance_billing_run_immutable BEFORE UPDATE OR DELETE ON finance_business_billing_runs FOR EACH ROW EXECUTE FUNCTION protect_finance_billing_run();
DO $$ DECLARE app_role TEXT:=regexp_replace(current_user,'_migration$','_app');BEGIN IF right(current_user,10)='_migration' AND EXISTS(SELECT 1 FROM pg_roles WHERE rolname=app_role) THEN EXECUTE format('GRANT UPDATE ON finance_business_billing_runs TO %I',app_role);END IF;END;$$;
