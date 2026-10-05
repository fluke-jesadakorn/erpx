-- Preserve original billing windows. Corrections form immutable retained editions.
ALTER TABLE finance_business_billing_runs ADD COLUMN parent_run_id TEXT REFERENCES finance_business_billing_runs(id);
ALTER TABLE finance_business_billing_runs ADD COLUMN parent_version BIGINT;
ALTER TABLE finance_business_billing_runs ADD COLUMN bill_edition BIGINT NOT NULL DEFAULT 1 CHECK(bill_edition>0);
ALTER TABLE finance_business_billing_runs ADD COLUMN contract_content_json TEXT CHECK(contract_content_json IS NULL OR erpx_json_valid(contract_content_json));
ALTER TABLE finance_business_billing_runs ADD COLUMN revision_reason TEXT;
ALTER TABLE finance_business_billing_runs ADD CONSTRAINT finance_billing_revision_shape CHECK((parent_run_id IS NULL AND parent_version IS NULL AND bill_edition=1 AND contract_content_json IS NULL AND revision_reason IS NULL) OR (parent_run_id IS NOT NULL AND parent_version>0 AND bill_edition>1 AND contract_content_json IS NOT NULL AND length(revision_reason)>=3));
ALTER TABLE finance_business_billing_runs DROP CONSTRAINT finance_business_billing_runs_root_id_billing_key_key;
CREATE UNIQUE INDEX finance_billing_first_window ON finance_business_billing_runs(root_id,billing_key) WHERE parent_run_id IS NULL;
CREATE UNIQUE INDEX finance_billing_revision_child ON finance_business_billing_runs(parent_run_id) WHERE parent_run_id IS NOT NULL;
CREATE OR REPLACE FUNCTION protect_finance_billing_run() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' OR OLD.invoice_id IS NOT NULL OR OLD.no_charge_status IN ('approved','rejected') OR (NEW.id,NEW.company_id,NEW.contract_id,NEW.root_id,NEW.contract_version,NEW.billing_key,NEW.service_from,NEW.service_to,NEW.input_json,NEW.result_json,NEW.source_hash,NEW.invoice_id,NEW.invoice_version,NEW.created_by,NEW.created_at,NEW.parent_run_id,NEW.parent_version,NEW.bill_edition,NEW.contract_content_json,NEW.revision_reason)
 IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.contract_id,OLD.root_id,OLD.contract_version,OLD.billing_key,OLD.service_from,OLD.service_to,OLD.input_json,OLD.result_json,OLD.source_hash,OLD.invoice_id,OLD.invoice_version,OLD.created_by,OLD.created_at,OLD.parent_run_id,OLD.parent_version,OLD.bill_edition,OLD.contract_content_json,OLD.revision_reason) THEN RAISE EXCEPTION 'BILLING_EVIDENCE_IMMUTABLE' USING ERRCODE='23514';END IF;
 IF EXISTS(SELECT 1 FROM finance_business_billing_runs child WHERE child.parent_run_id=OLD.id) THEN RAISE EXCEPTION 'BILLING_EDITION_SUPERSEDED' USING ERRCODE='23514';END IF;
 RETURN NEW;END;$$;
CREATE FUNCTION guard_finance_billing_revision() RETURNS trigger LANGUAGE plpgsql AS $$ DECLARE parent finance_business_billing_runs; BEGIN
 IF NEW.parent_run_id IS NULL THEN RETURN NEW;END IF;
 SELECT * INTO parent FROM finance_business_billing_runs WHERE id=NEW.parent_run_id;
 IF parent.id IS NULL OR parent.company_id<>NEW.company_id OR parent.contract_id<>NEW.contract_id OR parent.root_id<>NEW.root_id OR parent.billing_key<>NEW.billing_key OR parent.service_from<>NEW.service_from OR parent.service_to<>NEW.service_to OR parent.version<>NEW.parent_version OR parent.bill_edition+1<>NEW.bill_edition OR EXISTS(SELECT 1 FROM finance_recognition_runs r WHERE r.billing_run_id=parent.id AND r.status IN ('draft','posted')) THEN RAISE EXCEPTION 'BILLING_REVISION_SOURCE_CHANGED' USING ERRCODE='23514';END IF;
 IF parent.invoice_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM finance_invoices i WHERE i.id=parent.invoice_id AND i.company_id=parent.company_id AND i.status='voided' AND i.journal_id IS NULL AND i.posted_at IS NULL) THEN RAISE EXCEPTION 'BILLING_REVISION_REQUIRES_UNPOSTED_SOURCE' USING ERRCODE='23514';END IF;
 RETURN NEW;END;$$;
-- Owner void and replacement creation commit atomically; no posted invoice is changed.
CREATE CONSTRAINT TRIGGER finance_billing_revision_source AFTER INSERT ON finance_business_billing_runs DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION guard_finance_billing_revision();
