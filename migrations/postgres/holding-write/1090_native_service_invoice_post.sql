-- Field-origin invoice posting has one native writer. No data/history cleanup.
CREATE FUNCTION erpx_require_native_service_invoice_post() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE raw text; marker jsonb;
BEGIN
 IF (OLD.source_type='field_service' OR NEW.source_type='field_service') AND (
  (NEW.status='posted' AND OLD.status IS DISTINCT FROM NEW.status) OR
  ROW(NEW.journal_id,NEW.fx_rate_micros,NEW.base_total_minor,NEW.approved_by,NEW.posted_at,NEW.source_type,NEW.source_id,NEW.company_id,NEW.id,NEW.kind)
  IS DISTINCT FROM ROW(OLD.journal_id,OLD.fx_rate_micros,OLD.base_total_minor,OLD.approved_by,OLD.posted_at,OLD.source_type,OLD.source_id,OLD.company_id,OLD.id,OLD.kind)
 ) THEN
  raw:=current_setting('erpx.service_invoice_post_runtime',true);
  IF raw IS JSON OBJECT THEN marker:=raw::jsonb; ELSE marker:='{}'::jsonb; END IF;
  IF marker->>'database' IS DISTINCT FROM current_database()
   OR marker->>'pid' IS DISTINCT FROM pg_backend_pid()::text
   OR marker->>'xid' IS DISTINCT FROM pg_current_xact_id()::text
   OR marker->>'started' IS DISTINCT FROM pg_postmaster_start_time()::text
   OR marker->'invoice' IS DISTINCT FROM to_jsonb(NEW) THEN
   RAISE EXCEPTION 'Field invoice posting requires the native owner unit'
    USING ERRCODE='23514',CONSTRAINT='native_service_invoice_post_required';
  END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER native_service_invoice_post_required BEFORE UPDATE ON finance_invoices
 FOR EACH ROW EXECUTE FUNCTION erpx_require_native_service_invoice_post();
