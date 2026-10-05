-- A posted bill is retained. Its full credit, recognition reversals, replacement
-- and refund obligation are one reviewed owner transaction.
CREATE TABLE finance_billing_corrections (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL,
 billing_run_id TEXT NOT NULL REFERENCES finance_business_billing_runs(id),
 invoice_id TEXT NOT NULL REFERENCES finance_invoices(id),
 number TEXT NOT NULL, entry_date TEXT NOT NULL, period_id TEXT NOT NULL REFERENCES finance_periods(id),
 input_json TEXT NOT NULL CHECK(erpx_json_valid(input_json)),
 result_json TEXT NOT NULL CHECK(erpx_json_valid(result_json)),
 source_hash TEXT NOT NULL, source_json TEXT NOT NULL CHECK(erpx_json_valid(source_json)),
 subtotal_minor BIGINT NOT NULL CHECK(subtotal_minor>0), tax_minor BIGINT NOT NULL CHECK(tax_minor>=0),
 total_minor BIGINT NOT NULL CHECK(total_minor=subtotal_minor+tax_minor),
 credited_minor BIGINT CHECK(credited_minor>=0), refund_minor BIGINT CHECK(refund_minor>=0),
 credit_journal_id TEXT REFERENCES finance_journals(id),
 replacement_bill_id TEXT REFERENCES finance_business_billing_runs(id) DEFERRABLE INITIALLY DEFERRED,
 status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','posted','rejected')),
 version BIGINT NOT NULL DEFAULT 1, prepared_by TEXT NOT NULL, reviewed_by TEXT,
 reason TEXT NOT NULL, review_reason TEXT, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
 CHECK(reviewed_by IS NULL OR reviewed_by<>prepared_by),
 CHECK(status<>'posted' OR (credit_journal_id IS NOT NULL AND replacement_bill_id IS NOT NULL AND credited_minor+refund_minor=total_minor AND reviewed_by IS NOT NULL)),
 UNIQUE(company_id,number)
);
CREATE UNIQUE INDEX finance_billing_correction_current ON finance_billing_corrections(billing_run_id) WHERE status IN ('draft','posted');
CREATE FUNCTION protect_finance_billing_correction() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' OR OLD.status<>'draft' OR (to_jsonb(NEW)-ARRAY['status','version','reviewed_by','review_reason','updated_at','credit_journal_id','replacement_bill_id','credited_minor','refund_minor']) IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['status','version','reviewed_by','review_reason','updated_at','credit_journal_id','replacement_bill_id','credited_minor','refund_minor']) THEN RAISE EXCEPTION 'BILLING_CORRECTION_EVIDENCE_IMMUTABLE' USING ERRCODE='23514';END IF;RETURN NEW;END;$$;
CREATE TRIGGER finance_billing_correction_immutable BEFORE UPDATE OR DELETE ON finance_billing_corrections FOR EACH ROW EXECUTE FUNCTION protect_finance_billing_correction();
CREATE OR REPLACE FUNCTION guard_finance_billing_revision() RETURNS trigger LANGUAGE plpgsql AS $$ DECLARE parent finance_business_billing_runs; BEGIN
 IF NEW.parent_run_id IS NULL THEN RETURN NEW;END IF;
 SELECT * INTO parent FROM finance_business_billing_runs WHERE id=NEW.parent_run_id;
 IF parent.id IS NULL OR parent.company_id<>NEW.company_id OR parent.contract_id<>NEW.contract_id OR parent.root_id<>NEW.root_id OR parent.billing_key<>NEW.billing_key OR parent.service_from<>NEW.service_from OR parent.service_to<>NEW.service_to OR parent.version<>NEW.parent_version OR parent.bill_edition+1<>NEW.bill_edition OR EXISTS(SELECT 1 FROM finance_recognition_runs r WHERE r.billing_run_id=parent.id AND r.status IN ('draft','posted')) THEN RAISE EXCEPTION 'BILLING_REVISION_SOURCE_CHANGED' USING ERRCODE='23514';END IF;
 IF parent.invoice_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM finance_invoices i WHERE i.id=parent.invoice_id AND i.company_id=parent.company_id AND i.status='voided' AND i.journal_id IS NULL AND i.posted_at IS NULL) AND NOT EXISTS(SELECT 1 FROM finance_billing_corrections c JOIN finance_journals j ON j.id=c.credit_journal_id AND j.company_id=c.company_id WHERE c.company_id=parent.company_id AND c.billing_run_id=parent.id AND c.invoice_id=parent.invoice_id AND c.replacement_bill_id=NEW.id AND c.status='posted' AND j.status='posted' AND j.source_type='business_billing_credit' AND j.source_id=c.id) THEN RAISE EXCEPTION 'BILLING_REVISION_REQUIRES_OWNER_CREDIT' USING ERRCODE='23514';END IF;
 RETURN NEW;END;$$;
ALTER TABLE finance_refund_obligations ALTER COLUMN return_id DROP NOT NULL;
ALTER TABLE finance_refund_obligations ALTER COLUMN branch_id DROP NOT NULL;
ALTER TABLE finance_refund_obligations ADD COLUMN billing_correction_id TEXT UNIQUE REFERENCES finance_billing_corrections(id);
ALTER TABLE finance_refund_obligations ADD CONSTRAINT finance_refund_source_shape CHECK((return_id IS NOT NULL AND billing_correction_id IS NULL AND branch_id IS NOT NULL) OR (return_id IS NULL AND billing_correction_id IS NOT NULL));
ALTER TABLE finance_tax_entries DROP CONSTRAINT finance_tax_entries_source_type_check;
ALTER TABLE finance_tax_entries ADD CONSTRAINT finance_tax_entries_source_type_check CHECK(source_type IN ('invoice','commerce_return','billing_credit'));
-- Keep the existing replacement guard, adding the new source-family condition.
CREATE OR REPLACE FUNCTION validate_vat_replacement() RETURNS trigger LANGUAGE plpgsql AS $$ DECLARE previous finance_tax_entries; correction finance_vat_corrections; BEGIN
 IF NEW.correction_id IS NULL THEN RETURN NEW;END IF;
 SELECT * INTO previous FROM finance_tax_entries WHERE id=NEW.replaces_entry_id;
 SELECT * INTO correction FROM finance_vat_corrections WHERE id=NEW.correction_id;
 IF previous.status IS DISTINCT FROM 'approved' OR correction.status IS DISTINCT FROM 'prepared' OR (NEW.company_id,NEW.chart_id,NEW.financial_event_id,NEW.tax_account_id,NEW.source_type,NEW.source_id,NEW.direction,NEW.document_date,NEW.accounting_date,NEW.taxable_minor,NEW.tax_minor) IS DISTINCT FROM (previous.company_id,previous.chart_id,previous.financial_event_id,previous.tax_account_id,previous.source_type,previous.source_id,previous.direction,previous.document_date,previous.accounting_date,previous.taxable_minor,previous.tax_minor) OR (NEW.company_id,NEW.chart_id,NEW.prepared_by) IS DISTINCT FROM (correction.company_id,correction.chart_id,correction.prepared_by) OR (NEW.source_type='invoice' AND NEW.source_id<>correction.family_source_id) OR (NEW.source_type='commerce_return' AND NOT EXISTS(SELECT 1 FROM commerce_returns WHERE id=NEW.source_id AND company_id=NEW.company_id AND invoice_id=correction.family_source_id)) OR (NEW.source_type='billing_credit' AND NOT EXISTS(SELECT 1 FROM finance_billing_corrections WHERE id=NEW.source_id AND company_id=NEW.company_id AND invoice_id=correction.family_source_id)) OR EXISTS(SELECT 1 FROM finance_tax_entries successor WHERE successor.replaces_entry_id=previous.id AND successor.status='approved') THEN RAISE EXCEPTION 'VAT_REPLACEMENT_SOURCE_INVALID' USING ERRCODE='23514';END IF;RETURN NEW;END;$$;
INSERT INTO work_cache_revisions(table_name) VALUES('finance_billing_corrections') ON CONFLICT DO NOTHING;
CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON finance_billing_corrections FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON finance_billing_corrections FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed();
DO $$ DECLARE app_role TEXT:=regexp_replace(current_user,'_migration$','_app');BEGIN IF right(current_user,10)='_migration' AND EXISTS(SELECT 1 FROM pg_roles WHERE rolname=app_role) THEN EXECUTE format('GRANT SELECT,INSERT,UPDATE ON finance_billing_corrections TO %I',app_role);END IF;END;$$;
