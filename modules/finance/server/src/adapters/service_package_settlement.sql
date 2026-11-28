WITH scope AS(SELECT $1::text company_id,$3::boolean is_base),
orders AS MATERIALIZED(SELECT * FROM jsonb_to_recordset($2::jsonb->'orders') AS o(id text,company_id text)),
releases AS MATERIALIZED(SELECT * FROM jsonb_to_recordset($2::jsonb->'releases') AS b(id text,order_id text,digest text)),
parts AS MATERIALIZED(SELECT * FROM jsonb_to_recordset($2::jsonb->'parts') AS p(id text,company_id text,source_id text,journal_id text,currency text,value_minor bigint,digest text)),
invoice_origins AS MATERIALIZED(SELECT * FROM jsonb_to_recordset($2::jsonb->'invoice_origins') AS p(id text,company_id text,release_id text,order_id text,customer_id text,currency text,amount_minor bigint,pricing_date text)),
parts_origins AS MATERIALIZED(SELECT * FROM jsonb_to_recordset($2::jsonb->'parts_origins') AS p(company_id text,journal_id text,consumption_id text)),
actual_invoice AS MATERIALIZED(
 SELECT i.company_id,i.id,origin.release_id money_source_id FROM finance_invoices i JOIN invoice_origins origin
  ON origin.id=i.id AND origin.company_id=i.company_id AND i.kind='receivable' AND i.source_type='field_service'
  AND i.source_id=origin.order_id AND i.party_id=origin.customer_id AND i.currency=origin.currency
  AND i.total_minor=origin.amount_minor AND i.invoice_date=origin.pricing_date
 JOIN money_transaction_snapshots m ON m.company_id=i.company_id AND m.source_type='field_service_billing_release'
  AND m.source_id=origin.release_id AND m.amount_kind='invoice_total' AND m.currency=i.currency AND m.amount_units=i.total_minor
),actual_parts AS MATERIALIZED(
 SELECT j.company_id,j.id FROM finance_journals j JOIN parts_origins origin ON origin.journal_id=j.id AND origin.company_id=j.company_id
 JOIN parts p ON p.id=origin.consumption_id AND p.company_id=j.company_id AND j.source_type='field_service_parts'
  AND j.source_id=p.id AND j.currency=p.currency AND p.value_minor>0
 JOIN money_transaction_snapshots m ON m.company_id=j.company_id AND m.source_type='operational_inventory_consumption'
  AND m.source_id=p.id AND m.amount_kind='stock_cost' AND m.currency=p.currency AND m.amount_units=p.value_minor
 WHERE(SELECT sum(debit_minor) FROM finance_journal_lines WHERE journal_id=j.id)=p.value_minor
  AND(SELECT sum(credit_minor) FROM finance_journal_lines WHERE journal_id=j.id)=p.value_minor
),invoices AS MATERIALIZED(
 SELECT i.* FROM finance_invoices i,scope WHERE scope.is_base AND(
  i.company_id=scope.company_id AND i.source_type='field_service'
  OR EXISTS(SELECT 1 FROM actual_invoice owned WHERE owned.id=i.id AND owned.company_id=scope.company_id)
  OR erpx_finance_field_invoice_owned(i.id,scope.company_id))
),completed_no_charge AS MATERIALIZED(
 -- Keep the original complete decision proof intact, but plan it once behind
 -- an owner-local boundary. Repeated settlement guards must not inline this
 -- multi-source historical view into every correlated invoice debt branch.
 SELECT complete.id,complete.company_id FROM finance_service_no_charge_completed complete
 WHERE complete.company_id=$1 AND EXISTS(SELECT 1 FROM invoices invoice
  WHERE invoice.id=complete.id AND invoice.company_id=complete.company_id AND invoice.status='no_charge_confirmed')
 OFFSET 0
),payments AS MATERIALIZED(
 SELECT p.* FROM finance_payments p,scope WHERE scope.is_base AND(
  EXISTS(SELECT 1 FROM finance_allocations a JOIN invoices i ON i.id=a.invoice_id WHERE a.payment_id=p.id)
  OR EXISTS(SELECT 1 FROM command_receipts r JOIN invoices i ON i.id=r.result_json::jsonb->>'settlementInvoiceId' WHERE r.record_id=p.id AND r.operation='finance.payment.create'))
),actual_payment AS MATERIALIZED(
 SELECT p.company_id,p.id,source.id invoice_id FROM finance_payments p
 JOIN command_receipts r ON r.record_id=p.id AND r.operation='finance.payment.create' AND r.actor_id=p.created_by
  AND r.authority_json::jsonb->>'companyId'=p.company_id AND r.result_json::jsonb->>'id'=p.id
 JOIN actual_invoice source ON source.id=r.result_json::jsonb->>'settlementInvoiceId' AND source.company_id=p.company_id
 JOIN finance_invoices original ON original.id=source.id AND original.company_id=p.company_id AND original.party_id=p.party_id AND original.currency=p.currency
 WHERE p.kind='receipt' AND NOT EXISTS(SELECT 1 FROM finance_allocations a WHERE a.payment_id=p.id AND(a.company_id<>p.company_id OR a.invoice_id<>source.id))
 AND(p.status NOT IN('settled','voided') OR EXISTS(SELECT 1 FROM finance_allocations a WHERE a.payment_id=p.id AND a.company_id=p.company_id AND a.invoice_id=source.id HAVING sum(a.amount_minor)=p.amount_minor))
),journals AS MATERIALIZED(
 SELECT j.*,(SELECT count(*) FROM finance_journal_lines l WHERE l.journal_id=j.id) line_count,
 (SELECT sum(l.debit_minor-l.credit_minor) FROM finance_journal_lines l WHERE l.journal_id=j.id) imbalance,
 (SELECT sum(l.base_debit_minor-l.base_credit_minor) FROM finance_journal_lines l WHERE l.journal_id=j.id) base_imbalance FROM finance_journals j
),owned_journals AS MATERIALIZED(
 SELECT p.company_id,p.journal_id id,'field_service_parts' kind,p.id source_id,p.currency,p.value_minor amount FROM parts p
 UNION SELECT i.company_id,i.journal_id,'invoice',i.id,i.currency,i.total_minor FROM invoices i WHERE i.journal_id IS NOT NULL
 UNION SELECT p.company_id,owned.id,owned.kind,p.id,p.currency,NULL::bigint FROM payments p CROSS JOIN LATERAL(VALUES(p.journal_id,'payment'),(p.fx_journal_id,'payment_fx')) owned(id,kind) WHERE owned.id IS NOT NULL
 UNION SELECT p.company_id,owned.id,'reversal',p.id,p.currency,NULL::bigint FROM payments p JOIN finance_payment_reversals r ON r.payment_id=p.id CROSS JOIN LATERAL(VALUES(r.reversal_journal_id),(r.fx_reversal_journal_id)) owned(id) WHERE owned.id IS NOT NULL
),facts AS(
 SELECT 'release' kind,release.id,release.digest fact FROM releases release JOIN orders source ON source.id=release.order_id CROSS JOIN scope WHERE scope.is_base AND NOT EXISTS(SELECT 1 FROM actual_invoice invoice WHERE invoice.money_source_id=release.id AND invoice.company_id=source.company_id)
 UNION ALL SELECT 'invoice',invoice.id,to_jsonb(invoice)::text FROM invoices invoice,scope WHERE invoice.company_id<>scope.company_id OR NOT EXISTS(SELECT 1 FROM actual_invoice owned WHERE owned.id=invoice.id AND owned.company_id=invoice.company_id) OR (NOT (invoice.status='no_charge_confirmed' AND EXISTS(SELECT 1 FROM completed_no_charge n WHERE n.id=invoice.id AND n.company_id=invoice.company_id)) AND (invoice.status<>'paid' OR invoice.outstanding_minor<>0 OR invoice.journal_id IS NULL OR NOT EXISTS(SELECT 1 FROM money_transaction_snapshots money WHERE money.company_id=invoice.company_id AND money.source_type='finance_invoice' AND money.source_id=invoice.id AND money.amount_kind='total' AND money.currency=invoice.currency AND money.amount_units=invoice.total_minor)))
 UNION ALL SELECT 'issue-capture',part.id,to_jsonb(part)::text FROM parts part WHERE NOT EXISTS(SELECT 1 FROM actual_parts owned WHERE owned.id=part.journal_id AND owned.company_id=part.company_id)
 UNION ALL SELECT 'payment',payment.id,to_jsonb(payment)::text FROM payments payment,scope WHERE payment.company_id<>scope.company_id OR payment.status NOT IN ('settled','voided') OR payment.status='settled' AND (payment.outstanding_minor<>0 OR payment.journal_id IS NULL OR NOT EXISTS(SELECT 1 FROM actual_payment owned WHERE owned.id=payment.id AND owned.company_id=payment.company_id) OR NOT EXISTS(SELECT 1 FROM finance_bank_transactions bank WHERE bank.company_id=payment.company_id AND bank.matched_type='payment' AND bank.matched_id=payment.id AND bank.match_status='matched' AND bank.amount_minor=payment.amount_minor AND bank.currency=payment.currency) OR NOT EXISTS(SELECT 1 FROM money_transaction_snapshots money WHERE money.company_id=payment.company_id AND money.source_type='finance_payment' AND money.source_id=payment.id AND money.amount_kind='settled' AND money.amount_units=payment.amount_minor AND money.currency=payment.currency)) OR payment.status='voided' AND NOT EXISTS(SELECT 1 FROM finance_payment_reversals reversal WHERE reversal.payment_id=payment.id)
 UNION ALL SELECT 'source-bank',bank.id,to_jsonb(bank)::text FROM finance_bank_transactions bank JOIN command_receipts receipt ON receipt.record_id=bank.statement_id AND receipt.operation='finance.bank.import' JOIN payments payment ON payment.id=receipt.result_json::jsonb->>'settlementPaymentId' WHERE bank.company_id<>payment.company_id OR bank.match_status<>'matched' OR bank.matched_type IS DISTINCT FROM 'payment' OR bank.matched_id IS DISTINCT FROM payment.id
 UNION ALL SELECT 'refund',refund.id,to_jsonb(refund)::text FROM finance_refund_obligations refund JOIN invoices invoice ON invoice.id=refund.invoice_id WHERE refund.company_id<>invoice.company_id OR refund.status<>'settled' OR refund.outstanding_minor<>0 OR NOT EXISTS(SELECT 1 FROM finance_refund_instructions instruction JOIN finance_payments payment ON payment.id=instruction.payment_id AND payment.company_id=refund.company_id JOIN finance_bank_transactions bank ON bank.id=instruction.bank_transaction_id AND bank.company_id=refund.company_id WHERE instruction.obligation_id=refund.id AND instruction.status='confirmed' AND payment.status='settled' AND bank.match_status='matched')
 UNION ALL SELECT 'refund-instruction',instruction.id,to_jsonb(instruction)::text FROM finance_refund_instructions instruction JOIN finance_refund_obligations refund ON refund.id=instruction.obligation_id JOIN invoices invoice ON invoice.id=refund.invoice_id WHERE instruction.company_id<>invoice.company_id OR instruction.status NOT IN ('confirmed','cancelled')
 UNION ALL SELECT 'billing-correction',correction.id,to_jsonb(correction)::text FROM finance_billing_corrections correction JOIN invoices invoice ON invoice.id=correction.invoice_id WHERE correction.company_id<>invoice.company_id OR correction.status NOT IN ('posted','rejected')
 UNION ALL SELECT 'journal',owned.id,to_jsonb(owned)::text FROM owned_journals owned LEFT JOIN journals journal ON journal.id=owned.id AND journal.company_id=owned.company_id WHERE journal.id IS NULL OR journal.status NOT IN ('posted','reversed') OR journal.line_count<2 OR journal.imbalance IS DISTINCT FROM 0 OR journal.base_imbalance IS DISTINCT FROM 0 OR owned.kind NOT IN ('payment_fx','reversal') AND journal.currency<>owned.currency OR owned.kind<>'reversal' AND (journal.source_type<>owned.kind OR journal.source_id IS DISTINCT FROM owned.source_id) OR owned.amount IS NOT NULL AND (SELECT sum(line.debit_minor) FROM finance_journal_lines line WHERE line.journal_id=journal.id)<>owned.amount
 UNION ALL SELECT 'secondary',book.secondary_journal_id,to_jsonb(book)::text FROM finance_book_postings book JOIN owned_journals owned ON owned.id=book.primary_journal_id LEFT JOIN journals primary_journal ON primary_journal.id=owned.id LEFT JOIN journals secondary ON secondary.id=book.secondary_journal_id AND secondary.company_id=owned.company_id LEFT JOIN finance_book_rules rule ON rule.id=book.rule_id WHERE book.company_id<>owned.company_id OR secondary.id IS NULL OR secondary.status NOT IN ('posted','reversed') OR secondary.line_count<2 OR secondary.imbalance IS DISTINCT FROM 0 OR secondary.base_imbalance IS DISTINCT FROM 0 OR rule.company_id IS DISTINCT FROM owned.company_id OR rule.status<>'approved' OR rule.reviewed_by IS NULL OR rule.reviewed_by=rule.prepared_by OR secondary.chart_id<>rule.secondary_chart_id OR primary_journal.chart_id<>rule.primary_chart_id OR secondary.currency<>primary_journal.currency

) SELECT count(*)::bigint outstanding,encode(sha256(convert_to(coalesce(jsonb_agg(to_jsonb(f) ORDER BY kind,id,fact)::text,'[]'),'UTF8')),'hex') revision FROM facts f
