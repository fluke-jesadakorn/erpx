import 'server-only';
/** Original Finance Treasury receipts retain their exact immutable issued copy,
 * command and single invoice allocation. The consuming Commerce owner must
 * separately resolve this invoice through its actual Sales relationship. */
export const financeSalesReceiptOriginalSources=`SELECT p.id,p.company_id,p.party_id,p.currency,p.amount_minor,p.bank_account,p.journal_id,p.created_by,p.status,invoice.id invoice_id,
 jsonb_build_array(to_jsonb(receipt),to_jsonb(created),to_jsonb(allocation),
 jsonb_build_array(p.id,p.company_id,p.party_id,p.kind,p.currency,p.amount_minor,p.bank_account,p.journal_id,p.created_by,p.created_at),
 jsonb_build_array(invoice.id,invoice.company_id,invoice.party_id,invoice.kind,invoice.currency,invoice.total_minor,invoice.source_type,invoice.source_id))::text image
 FROM finance_payments p JOIN finance_customer_receipts receipt ON receipt.payment_id=p.id AND receipt.company_id=p.company_id AND receipt.currency=p.currency AND receipt.amount_minor=p.amount_minor AND receipt.issued_by=p.created_by
 JOIN finance_allocations allocation ON allocation.payment_id=p.id AND allocation.company_id=p.company_id AND allocation.amount_minor=p.amount_minor
 JOIN finance_invoices invoice ON invoice.id=allocation.invoice_id AND invoice.company_id=p.company_id AND invoice.party_id=p.party_id AND invoice.currency=p.currency AND invoice.kind='receivable'
 JOIN command_receipts created ON created.record_id=invoice.id AND created.operation='commerce.invoice.settle' AND created.actor_id=p.created_by AND created.authority_json::jsonb->>'companyId'=p.company_id AND created.result_json::jsonb->>'paymentId'=p.id
 JOIN finance_journals journal ON journal.id=p.journal_id AND journal.company_id=p.company_id AND journal.source_type='commerce_payment' AND journal.source_id=p.id AND journal.currency=p.currency AND journal.status IN ('posted','reversed')
 JOIN finance_financial_events captured ON captured.journal_id=journal.id AND captured.company_id=p.company_id AND captured.source_type='commerce_payment' AND captured.source_id=p.id AND captured.legacy_snapshot=0
 JOIN money_transaction_snapshots money ON money.company_id=p.company_id AND money.source_type='finance_payment' AND money.source_id=p.id AND money.amount_kind='settled' AND money.currency=p.currency AND money.currency_scale=2 AND money.amount_units=p.amount_minor
 WHERE p.kind='receipt' AND p.amount_minor>0 AND ((p.status='settled' AND journal.status='posted') OR (p.status='voided' AND journal.status='reversed'))
 AND receipt.snapshot_json::jsonb->>'paymentId'=p.id AND receipt.snapshot_json::jsonb->>'journalId'=p.journal_id
 AND jsonb_array_length(receipt.snapshot_json::jsonb->'allocations')=1
 AND receipt.snapshot_json::jsonb->'allocations' @> jsonb_build_array(jsonb_build_object('invoiceId',invoice.id,'amountMinor',allocation.amount_minor::text))
 AND captured.evidence_json::jsonb->'lines'=(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.ordinal) FROM finance_journal_lines line WHERE line.journal_id=journal.id)
 AND NOT EXISTS(SELECT 1 FROM finance_allocations other WHERE other.payment_id=p.id AND other.id<>allocation.id)`;
