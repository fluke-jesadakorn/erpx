-- Protect both sides of reassignment and every shared-ledger mutation path.
CREATE OR REPLACE FUNCTION pms_protect_finance() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE protected BOOLEAN := false;
DECLARE previous_row JSONB := '{}'::jsonb;
DECLARE next_row JSONB := '{}'::jsonb;
BEGIN
 IF TG_OP <> 'INSERT' THEN previous_row := to_jsonb(OLD); END IF;
 IF TG_OP <> 'DELETE' THEN next_row := to_jsonb(NEW); END IF;
 IF TG_TABLE_NAME IN ('finance_journals','finance_invoices') THEN
  protected := COALESCE(previous_row->>'source_type','')='pms' OR COALESCE(next_row->>'source_type','')='pms';
 ELSIF TG_TABLE_NAME='finance_journal_lines' THEN
  SELECT EXISTS(SELECT 1 FROM finance_journals WHERE id IN (previous_row->>'journal_id',next_row->>'journal_id') AND source_type='pms') INTO protected;
 ELSIF TG_TABLE_NAME='finance_payments' THEN
  SELECT EXISTS(SELECT 1 FROM pms_documents WHERE finance_id IN (previous_row->>'id',next_row->>'id') AND kind IN ('receipt','refund')) INTO protected;
 ELSIF TG_TABLE_NAME='finance_allocations' THEN
  SELECT EXISTS(SELECT 1 FROM finance_invoices WHERE id IN (previous_row->>'invoice_id',next_row->>'invoice_id') AND source_type='pms')
   OR EXISTS(SELECT 1 FROM pms_documents WHERE finance_id IN (previous_row->>'payment_id',next_row->>'payment_id') AND kind IN ('receipt','refund')) INTO protected;
 END IF;
 IF protected AND current_setting('erpx.pms_write',true) IS DISTINCT FROM 'on' THEN
  RAISE EXCEPTION 'PMS_OWNER_WORKFLOW_REQUIRED' USING ERRCODE='23514';
 END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; ELSE RETURN NEW; END IF;
END $$;
DROP TRIGGER pms_journal_owner ON finance_journals;
CREATE TRIGGER pms_journal_owner BEFORE INSERT OR UPDATE OR DELETE ON finance_journals FOR EACH ROW EXECUTE FUNCTION pms_protect_finance();
DROP TRIGGER pms_invoice_owner ON finance_invoices;
CREATE TRIGGER pms_invoice_owner BEFORE INSERT OR UPDATE OR DELETE ON finance_invoices FOR EACH ROW EXECUTE FUNCTION pms_protect_finance();
DROP TRIGGER pms_allocation_owner ON finance_allocations;
CREATE TRIGGER pms_allocation_owner BEFORE INSERT OR UPDATE OR DELETE ON finance_allocations FOR EACH ROW EXECUTE FUNCTION pms_protect_finance();
CREATE TRIGGER pms_journal_line_owner BEFORE INSERT OR UPDATE OR DELETE ON finance_journal_lines FOR EACH ROW EXECUTE FUNCTION pms_protect_finance();
