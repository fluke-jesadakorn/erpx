ALTER TABLE finance_refund_obligations ADD COLUMN base_amount_minor BIGINT;
ALTER TABLE finance_refund_obligations ADD COLUMN base_outstanding_minor BIGINT;
ALTER TABLE finance_refund_obligations ADD COLUMN source_credit_journal_id TEXT REFERENCES finance_journals(id);
ALTER TABLE finance_refund_obligations ADD CONSTRAINT refund_base_value_nonnegative CHECK(base_amount_minor>=0 AND base_outstanding_minor>=0 AND base_outstanding_minor<=base_amount_minor);
ALTER TABLE commerce_returns ADD COLUMN base_credit_minor BIGINT;
ALTER TABLE commerce_returns ADD COLUMN base_credited_minor BIGINT;
ALTER TABLE commerce_returns ADD COLUMN base_refund_minor BIGINT;
ALTER TABLE commerce_returns ADD COLUMN fx_journal_id TEXT REFERENCES finance_journals(id);
ALTER TABLE commerce_returns ADD CONSTRAINT commerce_return_base_nonnegative CHECK(base_credit_minor>=0 AND base_credited_minor>=0 AND base_refund_minor>=0);
-- Existing two-decimal functional-currency evidence retains its original meaning.
-- Foreign legacy sources remain nullable until their owner reconciles the actual journals.
UPDATE commerce_returns r SET base_credit_minor=r.total_minor,base_credited_minor=r.credited_minor,base_refund_minor=r.refund_minor
FROM finance_invoices i WHERE i.id=r.invoice_id AND i.fx_rate_micros=1000000 AND r.status IN ('approved','posted');
UPDATE finance_refund_obligations o SET base_amount_minor=o.amount_minor,base_outstanding_minor=o.outstanding_minor,source_credit_journal_id=r.credit_journal_id
FROM commerce_returns r JOIN finance_invoices i ON i.id=r.invoice_id WHERE o.return_id=r.id AND i.fx_rate_micros=1000000;
ALTER TABLE ecommerce_listings DROP CONSTRAINT ecommerce_listings_status_check;
ALTER TABLE ecommerce_listings ADD CONSTRAINT ecommerce_listings_status_check CHECK(status IN ('draft','published','rejected','retired'));
CREATE FUNCTION commerce_return_carrying_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF OLD.status='posted' AND (NEW.base_credit_minor,NEW.base_credited_minor,NEW.base_refund_minor,NEW.fx_journal_id) IS DISTINCT FROM (OLD.base_credit_minor,OLD.base_credited_minor,OLD.base_refund_minor,OLD.fx_journal_id)
 THEN RAISE EXCEPTION 'COMMERCE_RETURN_CARRYING_IMMUTABLE' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER commerce_return_carrying_guard BEFORE UPDATE ON commerce_returns FOR EACH ROW EXECUTE FUNCTION commerce_return_carrying_guard();
CREATE FUNCTION finance_refund_source_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (NEW.company_id,NEW.return_id,NEW.invoice_id,NEW.party_id,NEW.direction,NEW.currency,NEW.chart_id,NEW.balance_account_id,NEW.amount_minor,NEW.base_amount_minor,NEW.source_credit_journal_id)
    IS DISTINCT FROM (OLD.company_id,OLD.return_id,OLD.invoice_id,OLD.party_id,OLD.direction,OLD.currency,OLD.chart_id,OLD.balance_account_id,OLD.amount_minor,OLD.base_amount_minor,OLD.source_credit_journal_id)
 THEN RAISE EXCEPTION 'FINANCE_REFUND_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF NEW.base_outstanding_minor IS DISTINCT FROM OLD.base_outstanding_minor AND (
   NEW.outstanding_minor<>NEW.amount_minor-COALESCE((SELECT SUM(i.amount_minor) FROM finance_refund_instructions i WHERE i.obligation_id=NEW.id AND i.status='confirmed'),0)
   OR NEW.base_outstanding_minor<>NEW.base_amount_minor-COALESCE((SELECT SUM(i.base_carrying_minor) FROM finance_refund_instructions i WHERE i.obligation_id=NEW.id AND i.status='confirmed'),0))
 THEN RAISE EXCEPTION 'FINANCE_REFUND_CARRYING_EVIDENCE_REQUIRED' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER finance_refund_source_guard BEFORE UPDATE ON finance_refund_obligations FOR EACH ROW EXECUTE FUNCTION finance_refund_source_guard();
