-- 1033/1034 permit original warehouse-free Company Sales services and credits.
-- Keep the two exclusive Refund owners. A branchless Return additionally needs
-- the exact native service/credit/completion proof below; a source label is not
-- a Goods exception, a Billing correction or permission to invent a Branch.
ALTER TABLE finance_refund_obligations DROP CONSTRAINT finance_refund_source_shape;
ALTER TABLE finance_refund_obligations ADD CONSTRAINT finance_refund_source_shape
 CHECK((return_id IS NOT NULL AND billing_correction_id IS NULL)
    OR (return_id IS NULL AND billing_correction_id IS NOT NULL));

CREATE FUNCTION finance_company_service_refund_source_valid(refund_id TEXT)
RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
 SELECT EXISTS(
  SELECT 1 FROM finance_refund_obligations refund
  JOIN commerce_returns returned ON returned.id=refund.return_id
   AND returned.company_id=refund.company_id AND returned.kind='customer_return'
   AND returned.branch_id IS NULL AND returned.status='posted'
   AND returned.invoice_id=refund.invoice_id
   AND returned.credit_journal_id=refund.source_credit_journal_id
   AND returned.refund_minor=refund.amount_minor
   AND returned.base_refund_minor=refund.base_amount_minor
   AND returned.stock_journal_id IS NULL AND returned.goods_fx_journal_id IS NULL
  JOIN commerce_orders ordered ON ordered.id=returned.order_id
   AND ordered.company_id=refund.company_id AND ordered.kind='sales'
   AND ordered.branch_id IS NULL AND ordered.department_id IS NULL
   AND ordered.warehouse_id IS NULL AND ordered.party_id=refund.party_id
   AND ordered.currency=refund.currency
  JOIN commerce_fulfillments completion ON completion.id=returned.fulfillment_id
   AND completion.order_id=ordered.id AND completion.invoice_id=refund.invoice_id
   AND completion.completion_kind='service' AND completion.journal_id IS NULL
  JOIN finance_parties customer ON customer.id=refund.party_id
   AND customer.company_id=refund.company_id AND customer.kind='customer'
  JOIN finance_invoices invoice ON invoice.id=refund.invoice_id
   AND invoice.company_id=refund.company_id AND invoice.kind='receivable'
   AND invoice.party_id=refund.party_id AND invoice.currency=refund.currency
   AND invoice.debit_account_id=refund.balance_account_id
   AND invoice.source_type='commerce_fulfillment'
   AND invoice.status IN ('posted','partially_paid','paid')
  -- An original invoice may combine several service completions of this order.
  JOIN commerce_fulfillments primary_completion ON primary_completion.id=invoice.source_id
   AND primary_completion.order_id=ordered.id AND primary_completion.invoice_id=invoice.id
   AND primary_completion.completion_kind='service' AND primary_completion.journal_id IS NULL
  JOIN finance_journals original_invoice_journal ON original_invoice_journal.id=invoice.journal_id
   AND original_invoice_journal.company_id=refund.company_id AND original_invoice_journal.chart_id=refund.chart_id
   AND original_invoice_journal.currency=refund.currency AND original_invoice_journal.status='posted'
   AND original_invoice_journal.source_type='invoice' AND original_invoice_journal.source_id=invoice.id
  JOIN finance_accounts control ON control.id=refund.balance_account_id
   AND control.company_id=refund.company_id AND control.chart_id=refund.chart_id
   AND control.category='asset'
  JOIN finance_charts chart ON chart.id=refund.chart_id AND chart.company_id=refund.company_id
  JOIN finance_journals credit ON credit.id=refund.source_credit_journal_id
   AND credit.company_id=refund.company_id AND credit.chart_id=refund.chart_id
   AND credit.currency=refund.currency AND credit.status='posted'
   AND credit.source_type='credit_note' AND credit.source_id=returned.id
  JOIN command_receipts receipt ON receipt.operation='commerce.return.complete'
   AND receipt.record_id=returned.id AND receipt.actor_id=refund.created_by
   AND receipt.authority_json::jsonb->>'companyId'=refund.company_id
   AND receipt.authority_json::jsonb->>'branchId' IS NULL
   AND receipt.authority_json::jsonb->>'departmentId' IS NULL
   AND receipt.result_json::jsonb->>'id'=returned.id
   AND receipt.result_json::jsonb->>'orderId'=ordered.id
   AND receipt.result_json::jsonb->>'status'='posted'
   AND receipt.result_json::jsonb->>'version'=returned.version::text
   AND receipt.result_json::jsonb->>'refundId'=refund.id
   AND receipt.result_json::jsonb->>'refundMinor'=refund.amount_minor::text
  WHERE refund.id=refund_id AND refund.return_id IS NOT NULL
   AND refund.billing_correction_id IS NULL AND refund.branch_id IS NULL
   AND refund.department_id IS NULL AND refund.direction='outgoing'
   AND refund.base_amount_minor IS NOT NULL AND refund.source_credit_journal_id IS NOT NULL
   AND EXISTS(SELECT 1 FROM commerce_order_lines line WHERE line.order_id=ordered.id)
   AND NOT EXISTS(SELECT 1 FROM commerce_order_lines line
    LEFT JOIN commerce_products product ON product.id=line.product_id
    WHERE line.order_id=ordered.id AND (line.item_type<>'service'
     OR product.company_id IS DISTINCT FROM ordered.company_id OR product.item_type IS DISTINCT FROM 'service'))
   AND EXISTS(SELECT 1 FROM commerce_fulfillment_lines line WHERE line.fulfillment_id=completion.id)
   AND NOT EXISTS(SELECT 1 FROM commerce_fulfillments sibling
    WHERE sibling.invoice_id=invoice.id AND (sibling.order_id<>ordered.id
     OR sibling.completion_kind<>'service' OR sibling.journal_id IS NOT NULL))
   AND NOT EXISTS(SELECT 1 FROM commerce_fulfillment_lines line
    JOIN commerce_fulfillments parent ON parent.id=line.fulfillment_id
    LEFT JOIN commerce_order_lines original ON original.id=line.order_line_id
    WHERE parent.invoice_id=invoice.id AND (original.order_id IS DISTINCT FROM ordered.id
     OR original.item_type IS DISTINCT FROM 'service' OR line.value_minor<>0))
   AND EXISTS(SELECT 1 FROM commerce_return_lines line WHERE line.return_id=returned.id)
   AND NOT EXISTS(SELECT 1 FROM commerce_return_lines line
    LEFT JOIN commerce_fulfillment_lines fulfilled ON fulfilled.id=line.fulfillment_line_id
    LEFT JOIN commerce_order_lines original ON original.id=fulfilled.order_line_id
    WHERE line.return_id=returned.id AND (fulfilled.fulfillment_id IS DISTINCT FROM completion.id
     OR original.order_id IS DISTINCT FROM ordered.id OR original.item_type IS DISTINCT FROM 'service'
     OR original.product_id IS DISTINCT FROM line.product_id OR line.value_minor<>0))
   AND NOT EXISTS(SELECT 1 FROM commerce_stock_moves movement WHERE movement.fulfillment_id=completion.id)
   AND NOT EXISTS(SELECT 1 FROM commerce_return_stock_moves movement WHERE movement.return_id=returned.id)
   AND NOT EXISTS(SELECT 1 FROM commerce_goods_valuations goods WHERE goods.fulfillment_id=completion.id)
   AND NOT EXISTS(SELECT 1 FROM commerce_invoice_goods_carrying goods WHERE goods.invoice_id=invoice.id)
 );
$$;

CREATE FUNCTION guard_finance_company_service_refund_source() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.return_id IS NOT NULL AND NEW.branch_id IS NULL
  AND NOT finance_company_service_refund_source_valid(NEW.id) THEN
  RAISE EXCEPTION 'FINANCE_COMPANY_SERVICE_REFUND_SOURCE_REQUIRED'
   USING ERRCODE='23514',CONSTRAINT='finance_refund_source_shape';
 END IF;
 RETURN NULL;
END;
$$;
-- The original completion updates the Return and writes its receipt in the same
-- native command. Validate its final state, including deferred owner effects.
CREATE CONSTRAINT TRIGGER finance_company_service_refund_source
 AFTER INSERT OR UPDATE ON finance_refund_obligations DEFERRABLE INITIALLY DEFERRED
 FOR EACH ROW EXECUTE FUNCTION guard_finance_company_service_refund_source();

CREATE FUNCTION protect_finance_company_service_refund_scope() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF OLD.return_id IS NOT NULL AND OLD.branch_id IS NULL
  AND (NEW.branch_id,NEW.department_id) IS DISTINCT FROM (OLD.branch_id,OLD.department_id) THEN
  RAISE EXCEPTION 'FINANCE_COMPANY_SERVICE_REFUND_SCOPE_IMMUTABLE' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END;
$$;
CREATE TRIGGER finance_company_service_refund_scope BEFORE UPDATE ON finance_refund_obligations
 FOR EACH ROW EXECUTE FUNCTION protect_finance_company_service_refund_scope();
