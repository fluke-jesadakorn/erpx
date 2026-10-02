CREATE FUNCTION protect_posted_invoice_amounts() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.journal_id IS NOT NULL AND OLD.status IN ('posted','partially_paid','paid','voided') AND
    (NEW.company_id,NEW.kind,NEW.invoice_number,NEW.invoice_date,NEW.due_date,NEW.currency,NEW.subtotal_minor,NEW.tax_minor,NEW.total_minor,NEW.tax_code,NEW.tax_account_id,NEW.debit_account_id,NEW.credit_account_id,NEW.source_type,NEW.source_id,NEW.journal_id,NEW.fx_rate_micros,NEW.base_total_minor)
    IS DISTINCT FROM
    (OLD.company_id,OLD.kind,OLD.invoice_number,OLD.invoice_date,OLD.due_date,OLD.currency,OLD.subtotal_minor,OLD.tax_minor,OLD.total_minor,OLD.tax_code,OLD.tax_account_id,OLD.debit_account_id,OLD.credit_account_id,OLD.source_type,OLD.source_id,OLD.journal_id,OLD.fx_rate_micros,OLD.base_total_minor)
    THEN RAISE EXCEPTION 'POSTED_INVOICE_EVIDENCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
  RETURN NEW;
END;
$$;
CREATE TRIGGER finance_invoice_posted_amounts BEFORE UPDATE ON finance_invoices
  FOR EACH ROW EXECUTE FUNCTION protect_posted_invoice_amounts();

-- Initial owner inserts may create a posted journal before its lines. Validate
-- the final transaction state, rather than forbidding those legitimate inserts.
CREATE FUNCTION verify_posted_journal_lines() RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public AS $$
DECLARE target TEXT; saved JSONB; actual JSONB;
BEGIN
  target:=CASE WHEN TG_OP='DELETE' THEN OLD.journal_id ELSE NEW.journal_id END;
  SELECT evidence_json::jsonb->'lines' INTO saved FROM finance_financial_events WHERE journal_id=target;
  IF saved IS NOT NULL THEN
    SELECT COALESCE(jsonb_agg(to_jsonb(l) ORDER BY l.ordinal),'[]'::jsonb) INTO actual FROM finance_journal_lines l WHERE journal_id=target;
    IF saved IS DISTINCT FROM actual THEN RAISE EXCEPTION 'POSTED_JOURNAL_LINES_IMMUTABLE' USING ERRCODE='23514'; END IF;
  END IF;
  RETURN NULL;
END;
$$;
CREATE CONSTRAINT TRIGGER finance_journal_lines_final_evidence AFTER INSERT OR UPDATE OR DELETE ON finance_journal_lines
  DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION verify_posted_journal_lines();
