-- Field-origin controlled receipt effects have one native writer. Classification
-- quarantines persisted Field markers, including malformed relationships; the
-- native consuming command independently proves complete current lineage.
-- No business rows, schema history or receipts are removed or backfilled.
CREATE FUNCTION erpx_finance_field_payment_owned(payment_ref text)
RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM command_receipts r JOIN finance_invoices i
   ON i.id=r.result_json::jsonb->>'settlementInvoiceId'
   WHERE r.operation='finance.payment.create'
   AND (r.record_id=payment_ref OR r.result_json::jsonb->>'id'=payment_ref)
   AND i.source_type='field_service')
 OR EXISTS(SELECT 1 FROM finance_allocations a JOIN finance_invoices i ON i.id=a.invoice_id
   WHERE a.payment_id=payment_ref AND i.source_type='field_service')
$$;
CREATE FUNCTION erpx_finance_field_statement_owned(statement_ref text)
RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM command_receipts r
   WHERE r.operation='finance.bank.import'
   AND (r.record_id=statement_ref OR r.result_json::jsonb->>'id'=statement_ref)
   AND erpx_finance_field_payment_owned(r.result_json::jsonb->>'settlementPaymentId'))
 OR EXISTS(SELECT 1 FROM finance_bank_transactions t WHERE t.statement_id=statement_ref
   AND t.matched_type='payment' AND erpx_finance_field_payment_owned(t.matched_id))
$$;
CREATE FUNCTION erpx_service_receipt_runtime_marker() RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE raw text; marker jsonb;
BEGIN
 raw:=current_setting('erpx.service_receipt_runtime',true);
 IF raw IS JSON OBJECT THEN marker:=raw::jsonb; ELSE RETURN '{}'::jsonb; END IF;
 IF marker->>'database' IS DISTINCT FROM current_database()
   OR marker->>'pid' IS DISTINCT FROM pg_backend_pid()::text
   OR marker->>'xid' IS DISTINCT FROM pg_current_xact_id()::text
   OR marker->>'started' IS DISTINCT FROM pg_postmaster_start_time()::text THEN RETURN '{}'::jsonb; END IF;
 RETURN marker;
END $$;
CREATE FUNCTION erpx_require_native_service_payment(payment_ref text) RETURNS void LANGUAGE plpgsql AS $$
DECLARE marker jsonb; actual jsonb; allocations jsonb;
BEGIN
 marker:=erpx_service_receipt_runtime_marker();
 SELECT to_jsonb(p) INTO actual FROM finance_payments p WHERE p.id=payment_ref;
 SELECT COALESCE(jsonb_agg(to_jsonb(a) ORDER BY a.id),'[]'::jsonb) INTO allocations
   FROM finance_allocations a WHERE a.payment_id=payment_ref;
 IF actual IS NULL OR marker->'payments'->payment_ref IS DISTINCT FROM actual
   OR marker->'allocations'->payment_ref IS DISTINCT FROM allocations THEN
   RAISE EXCEPTION 'Field receipt effects require the native owner unit'
     USING ERRCODE='23514',CONSTRAINT='native_service_payment_write_required';
 END IF;
END $$;
CREATE FUNCTION erpx_guard_native_service_payment() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP<>'INSERT' AND erpx_finance_field_payment_owned(OLD.id) THEN PERFORM erpx_require_native_service_payment(OLD.id); END IF;
 IF TG_OP<>'DELETE' AND erpx_finance_field_payment_owned(NEW.id) THEN PERFORM erpx_require_native_service_payment(NEW.id); END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; END IF; RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER native_service_payment_write_required AFTER INSERT OR UPDATE OR DELETE ON finance_payments
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION erpx_guard_native_service_payment();
CREATE FUNCTION erpx_guard_native_service_allocation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP<>'INSERT' AND (erpx_finance_field_payment_owned(OLD.payment_id) OR EXISTS(SELECT 1 FROM finance_invoices WHERE id=OLD.invoice_id AND source_type='field_service')) THEN PERFORM erpx_require_native_service_payment(OLD.payment_id); END IF;
 IF TG_OP<>'DELETE' AND (erpx_finance_field_payment_owned(NEW.payment_id) OR EXISTS(SELECT 1 FROM finance_invoices WHERE id=NEW.invoice_id AND source_type='field_service')) THEN PERFORM erpx_require_native_service_payment(NEW.payment_id); END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; END IF; RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER native_service_allocation_write_required AFTER INSERT OR UPDATE OR DELETE ON finance_allocations
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION erpx_guard_native_service_allocation();
CREATE FUNCTION erpx_guard_native_service_invoice_effect() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE marker jsonb; actual jsonb;
BEGIN
 IF OLD.source_type IS DISTINCT FROM 'field_service' AND NEW.source_type IS DISTINCT FROM 'field_service' THEN RETURN NEW; END IF;
 SELECT to_jsonb(i) INTO actual FROM finance_invoices i WHERE i.id=NEW.id;
 marker:=erpx_service_receipt_runtime_marker();
 IF actual IS NOT NULL AND marker->'invoices'->NEW.id=actual THEN RETURN NEW; END IF;
 RAISE EXCEPTION 'Field invoice updates require the native owner unit'
   USING ERRCODE='23514',CONSTRAINT='native_service_invoice_effect_required';
END $$;
CREATE CONSTRAINT TRIGGER native_service_invoice_effect_required AFTER UPDATE ON finance_invoices
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(OLD.* IS DISTINCT FROM NEW.*)
 EXECUTE FUNCTION erpx_guard_native_service_invoice_effect();
CREATE FUNCTION erpx_guard_native_service_bank_statement() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE candidate_id text; actual jsonb;
BEGIN
 FOR candidate_id IN SELECT DISTINCT v FROM unnest(ARRAY[CASE WHEN TG_OP<>'INSERT' THEN OLD.id END,CASE WHEN TG_OP<>'DELETE' THEN NEW.id END]) v WHERE v IS NOT NULL LOOP
   IF erpx_finance_field_statement_owned(candidate_id) THEN
     SELECT to_jsonb(s) INTO actual FROM finance_bank_statements s WHERE s.id=candidate_id;
     IF actual IS NULL OR erpx_service_receipt_runtime_marker()->'bankStatements'->candidate_id IS DISTINCT FROM actual THEN
       RAISE EXCEPTION 'Field bank evidence requires the native owner unit' USING ERRCODE='23514',CONSTRAINT='native_service_bank_write_required';
     END IF;
   END IF;
 END LOOP;
 IF TG_OP='DELETE' THEN RETURN OLD; END IF; RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER native_service_bank_statement_write_required AFTER INSERT OR UPDATE OR DELETE ON finance_bank_statements
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION erpx_guard_native_service_bank_statement();
CREATE FUNCTION erpx_guard_native_service_bank_row() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE marker jsonb; actual jsonb;
BEGIN
 IF (TG_OP<>'INSERT' AND (erpx_finance_field_statement_owned(OLD.statement_id) OR (OLD.matched_type='payment' AND erpx_finance_field_payment_owned(OLD.matched_id))))
 OR (TG_OP<>'DELETE' AND (erpx_finance_field_statement_owned(NEW.statement_id) OR (NEW.matched_type='payment' AND erpx_finance_field_payment_owned(NEW.matched_id)))) THEN
   IF TG_OP='DELETE' THEN RAISE EXCEPTION 'Field bank evidence cannot be removed by a legacy writer' USING ERRCODE='23514',CONSTRAINT='native_service_bank_write_required'; END IF;
   marker:=erpx_service_receipt_runtime_marker();
   SELECT to_jsonb(t) INTO actual FROM finance_bank_transactions t WHERE t.id=NEW.id;
   IF actual IS NULL OR marker->'bankRows'->NEW.id IS DISTINCT FROM actual THEN
     RAISE EXCEPTION 'Field bank evidence requires the native owner unit' USING ERRCODE='23514',CONSTRAINT='native_service_bank_write_required';
   END IF;
 END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; END IF; RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER native_service_bank_row_write_required AFTER INSERT OR UPDATE OR DELETE ON finance_bank_transactions
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION erpx_guard_native_service_bank_row();
