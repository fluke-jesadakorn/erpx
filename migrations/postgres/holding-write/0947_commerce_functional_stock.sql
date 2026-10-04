-- Document prices retain their currency; shared stock always carries chart-base money.
ALTER TABLE commerce_fulfillment_lines ADD COLUMN stock_currency TEXT;
ALTER TABLE commerce_return_lines ADD COLUMN expected_subtotal_minor BIGINT CHECK(expected_subtotal_minor>=0);
ALTER TABLE commerce_returns ADD COLUMN goods_fx_journal_id TEXT REFERENCES finance_journals(id);

-- Only unambiguous existing functional-currency sources are labelled. Unsupported
-- historical foreign goods stay nullable and require an owner reconciliation.
UPDATE commerce_fulfillment_lines l SET stock_currency=c.base_currency
FROM commerce_fulfillments f JOIN finance_journals j ON j.id=f.journal_id
JOIN finance_charts c ON c.id=j.chart_id JOIN commerce_orders o ON o.id=f.order_id
WHERE l.fulfillment_id=f.id AND j.currency=c.base_currency AND j.fx_rate_micros=1000000
AND o.currency=c.base_currency;
UPDATE commerce_return_lines l SET expected_subtotal_minor=l.value_minor
FROM commerce_returns r JOIN commerce_orders o ON o.id=r.order_id
JOIN commerce_fulfillments f ON f.id=r.fulfillment_id JOIN finance_journals j ON j.id=f.journal_id
JOIN finance_charts c ON c.id=j.chart_id
WHERE l.return_id=r.id AND r.kind='supplier_return' AND o.currency=c.base_currency
AND j.currency=c.base_currency AND j.fx_rate_micros=1000000;

CREATE TABLE commerce_goods_valuations (
 fulfillment_id TEXT PRIMARY KEY REFERENCES commerce_fulfillments(id),
 company_id TEXT NOT NULL,chart_id TEXT NOT NULL REFERENCES finance_charts(id),
 journal_id TEXT NOT NULL UNIQUE REFERENCES finance_journals(id),
 document_currency TEXT NOT NULL,journal_currency TEXT NOT NULL,stock_currency TEXT NOT NULL,
 valuation_date TEXT NOT NULL,fx_rate_micros BIGINT NOT NULL CHECK(fx_rate_micros>0),
 source_rate_id TEXT REFERENCES company_fx_rates(id),target_rate_id TEXT REFERENCES company_fx_rates(id),
 used_latest_fallback BOOLEAN NOT NULL,
 document_subtotal_minor BIGINT NOT NULL CHECK(document_subtotal_minor>=0),
 stock_value_minor BIGINT NOT NULL CHECK(stock_value_minor>=0),
 captured_by TEXT NOT NULL,captured_at BIGINT NOT NULL
);
CREATE TABLE commerce_invoice_goods_carrying (
 invoice_id TEXT PRIMARY KEY REFERENCES finance_invoices(id),company_id TEXT NOT NULL,
 chart_id TEXT NOT NULL REFERENCES finance_charts(id),stock_currency TEXT NOT NULL,
 clearing_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
 invoice_fx_rate_micros BIGINT NOT NULL CHECK(invoice_fx_rate_micros>0),
 invoice_rate_basis TEXT NOT NULL CHECK(invoice_rate_basis IN ('confirmed_company_quote','reviewed_input')),
 invoice_source_rate_id TEXT REFERENCES company_fx_rates(id),invoice_target_rate_id TEXT REFERENCES company_fx_rates(id),
 invoice_used_latest_fallback BOOLEAN NOT NULL,
 source_digest TEXT NOT NULL CHECK(length(source_digest)=64),source_json TEXT NOT NULL CHECK(erpx_json_valid(source_json)),
 receipt_base_minor BIGINT NOT NULL CHECK(receipt_base_minor>=0),
 invoice_clearing_base_minor BIGINT NOT NULL CHECK(invoice_clearing_base_minor>=0),
 adjustment_minor BIGINT NOT NULL,
 gain_account_id TEXT REFERENCES finance_accounts(id),loss_account_id TEXT REFERENCES finance_accounts(id),
 fx_journal_id TEXT REFERENCES finance_journals(id),reviewed_by TEXT NOT NULL,reviewed_at BIGINT NOT NULL,
 CHECK(adjustment_minor=invoice_clearing_base_minor-receipt_base_minor),
 CHECK((adjustment_minor=0)=(fx_journal_id IS NULL))
);
CREATE FUNCTION commerce_goods_evidence_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'COMMERCE_GOODS_EVIDENCE_IMMUTABLE' USING ERRCODE='23514'; END $$;
CREATE TRIGGER commerce_goods_valuation_immutable BEFORE UPDATE OR DELETE ON commerce_goods_valuations
FOR EACH ROW EXECUTE FUNCTION commerce_goods_evidence_immutable();
CREATE TRIGGER commerce_invoice_goods_carrying_immutable BEFORE UPDATE OR DELETE ON commerce_invoice_goods_carrying
FOR EACH ROW EXECUTE FUNCTION commerce_goods_evidence_immutable();
CREATE FUNCTION commerce_fulfillment_value_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF EXISTS(SELECT 1 FROM commerce_fulfillments f JOIN finance_journals j ON j.id=f.journal_id
   WHERE f.id=OLD.fulfillment_id AND j.status IN ('posted','reversed')) AND
   (TG_OP='DELETE' OR (NEW.id,NEW.fulfillment_id,NEW.order_line_id,NEW.quantity_milli,NEW.value_minor,NEW.subtotal_minor,NEW.tax_minor,NEW.stock_currency)
   IS DISTINCT FROM (OLD.id,OLD.fulfillment_id,OLD.order_line_id,OLD.quantity_milli,OLD.value_minor,OLD.subtotal_minor,OLD.tax_minor,OLD.stock_currency))
 THEN RAISE EXCEPTION 'COMMERCE_FULFILLMENT_VALUE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 RETURN CASE WHEN TG_OP='DELETE' THEN OLD ELSE NEW END;
END $$;
CREATE TRIGGER commerce_fulfillment_value_immutable BEFORE UPDATE OR DELETE ON commerce_fulfillment_lines
FOR EACH ROW EXECUTE FUNCTION commerce_fulfillment_value_immutable();
CREATE FUNCTION commerce_return_goods_fx_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF OLD.status='posted' AND NEW.goods_fx_journal_id IS DISTINCT FROM OLD.goods_fx_journal_id
 THEN RAISE EXCEPTION 'COMMERCE_RETURN_GOODS_FX_IMMUTABLE' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER commerce_return_goods_fx_immutable BEFORE UPDATE ON commerce_returns
FOR EACH ROW EXECUTE FUNCTION commerce_return_goods_fx_immutable();

CREATE FUNCTION commerce_goods_valuation_validate() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id
   JOIN finance_journals j ON j.id=f.journal_id JOIN finance_charts c ON c.id=j.chart_id
   WHERE f.id=NEW.fulfillment_id AND o.company_id=NEW.company_id AND j.company_id=NEW.company_id
   AND j.id=NEW.journal_id AND j.chart_id=NEW.chart_id AND c.company_id=NEW.company_id
   AND o.currency=NEW.document_currency AND j.currency=NEW.journal_currency AND c.base_currency=NEW.stock_currency
   AND f.fulfillment_date=NEW.valuation_date AND j.entry_date=NEW.valuation_date AND j.fx_rate_micros=NEW.fx_rate_micros
   AND NEW.document_subtotal_minor=(SELECT SUM(l.subtotal_minor) FROM commerce_fulfillment_lines l WHERE l.fulfillment_id=f.id)
   AND NEW.stock_value_minor=(SELECT SUM(l.value_minor) FROM commerce_fulfillment_lines l WHERE l.fulfillment_id=f.id)
   AND NOT EXISTS(SELECT 1 FROM commerce_fulfillment_lines l WHERE l.fulfillment_id=f.id AND l.stock_currency IS DISTINCT FROM NEW.stock_currency)
   AND NEW.stock_value_minor=(SELECT SUM(l.base_debit_minor) FROM finance_journal_lines l WHERE l.journal_id=j.id)
   AND (o.kind='purchase' AND j.currency=o.currency OR o.kind='sales' AND j.currency=c.base_currency AND j.fx_rate_micros=1000000))
 THEN RAISE EXCEPTION 'COMMERCE_GOODS_VALUATION_INVALID' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER commerce_goods_valuation_validate AFTER INSERT ON commerce_goods_valuations
DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION commerce_goods_valuation_validate();
