ALTER TABLE pms_import_balances ADD COLUMN finance_invoice_id TEXT UNIQUE REFERENCES finance_invoices(id);
-- Imported balances stay anchored to the ledger that Finance approved.
CREATE FUNCTION pms_opening_journal_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF EXISTS(SELECT 1 FROM pms_import_batches WHERE opening_journal_id=OLD.id AND state IN ('approved','applied'))
   AND (TG_OP='DELETE' OR NEW.status<>OLD.status) THEN
  RAISE EXCEPTION 'PMS_OPENING_JOURNAL_IN_USE';
 END IF;
 RETURN CASE WHEN TG_OP='DELETE' THEN OLD ELSE NEW END;
END $$;
CREATE TRIGGER pms_opening_journal_owner BEFORE UPDATE OR DELETE ON finance_journals
 FOR EACH ROW EXECUTE FUNCTION pms_opening_journal_guard();
CREATE FUNCTION pms_opening_invoice_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF EXISTS(SELECT 1 FROM pms_import_balances WHERE finance_invoice_id=OLD.id)
   AND COALESCE(current_setting('erpx.pms_write',true),'')<>'on' THEN
  RAISE EXCEPTION 'PMS_OWNER_WORKFLOW_REQUIRED';
 END IF;
 RETURN CASE WHEN TG_OP='DELETE' THEN OLD ELSE NEW END;
END $$;
CREATE TRIGGER pms_opening_invoice_owner BEFORE UPDATE OR DELETE ON finance_invoices
 FOR EACH ROW EXECUTE FUNCTION pms_opening_invoice_guard();
