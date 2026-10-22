import 'server-only';
import {supplierPaymentOriginalSources} from '@erpx/server-finance/adapters/supplier-payment-correction-sources';

/** A recorded independent no-payment decision is retained owner evidence, not
 * an external bank attestation. Only a proved complete replacement set closes this
 * original duty; unknown, partial, legacy and reversed sources stay pending.
 * The owner query supplies the Company-scoped settled statement CTE, using
 * the same complete statement proof as its raw statement obligation. */
export const failedSupplierInstructionSettlements=`SELECT failed.id,batch.company_id
 FROM finance_supplier_instructions failed
 JOIN finance_supplier_batches batch ON batch.id=failed.batch_id AND batch.status='exported'
 JOIN finance_invoices invoice ON invoice.id=failed.invoice_id AND invoice.company_id=batch.company_id AND invoice.currency=batch.currency
 JOIN command_receipts decision ON decision.operation='finance.supplier_batch.result' AND decision.record_id=failed.id AND decision.actor_id=failed.resolved_by
  AND decision.authority_json::jsonb->>'companyId'=batch.company_id AND decision.result_json::jsonb->>'id'=failed.id
  AND decision.authority_json::jsonb->>'assignmentId' IS NOT NULL AND decision.authority_json::jsonb->>'role' IS NOT NULL
  AND decision.result_json::jsonb->>'status'='failed' AND decision.result_json::jsonb->>'version'=failed.version::text
 WHERE failed.status='failed' AND failed.amount_minor>0 AND failed.payment_id IS NULL AND failed.bank_transaction_id IS NULL AND failed.bank_reference IS NULL
  AND length(trim(failed.evidence))>=3 AND failed.resolved_by<>batch.exported_by
  AND batch.created_by<>batch.approved_by AND batch.created_by<>batch.exported_by AND batch.approved_by<>batch.exported_by
  AND invoice.kind='payable' AND invoice.status='paid' AND invoice.total_minor>0 AND invoice.outstanding_minor=0 AND invoice.base_outstanding_minor=0
  AND invoice.fx_rate_micros=1000000 AND invoice.base_total_minor=invoice.total_minor
  AND failed.amount_minor=invoice.total_minor
  AND (SELECT count(*)>0 AND sum(allocation.amount_minor)=invoice.total_minor AND sum(allocation.base_invoice_minor)=invoice.base_total_minor AND sum(allocation.base_payment_minor)=invoice.base_total_minor FROM finance_allocations allocation WHERE allocation.invoice_id=invoice.id)
  AND NOT EXISTS(SELECT 1 FROM finance_allocations allocation WHERE allocation.invoice_id=invoice.id AND NOT (EXISTS(
   SELECT 1 FROM (${supplierPaymentOriginalSources}) replacement
   JOIN command_receipts replacement_created ON replacement_created.operation='finance.supplier_batch.create' AND replacement_created.record_id=replacement.batch_id
    AND replacement_created.authority_json::jsonb->>'companyId'=batch.company_id AND replacement_created.created_at>=decision.created_at
   JOIN finance_bank_transactions movement ON movement.id=replacement.bank_transaction_id AND movement.company_id=batch.company_id
    AND movement.match_status='matched' AND movement.matched_type='payment' AND movement.matched_id=replacement.id
   JOIN settled_finance_bank_statements statement ON statement.id=movement.statement_id AND statement.company_id=batch.company_id AND statement.bank_account=batch.bank_account_id
   WHERE allocation.company_id=invoice.company_id AND replacement.id=allocation.payment_id AND replacement.invoice_id=invoice.id AND replacement.company_id=invoice.company_id
    AND replacement.batch_id<>batch.id AND replacement.instruction_id<>failed.id AND replacement.status='settled'
    AND replacement.amount_minor=allocation.amount_minor AND replacement.bank_account=batch.bank_account_id AND replacement.currency=batch.currency)
   OR EXISTS(SELECT 1 FROM finance_payments replacement
    JOIN command_receipts replacement_created ON replacement_created.operation='finance.payment.create' AND replacement_created.record_id=replacement.id AND replacement_created.actor_id=replacement.created_by
     AND replacement_created.authority_json::jsonb->>'companyId'=invoice.company_id AND replacement_created.result_json::jsonb->>'id'=replacement.id
     AND replacement_created.result_json::jsonb->>'status'='draft'
     AND replacement_created.result_json::jsonb->>'correctionPaymentId' IS NULL AND replacement_created.created_at>=decision.created_at
    JOIN finance_bank_transactions movement ON movement.matched_id=replacement.id AND movement.company_id=invoice.company_id AND movement.match_status='matched' AND movement.matched_type='payment'
    JOIN settled_finance_bank_statements statement ON statement.id=movement.statement_id AND statement.company_id=invoice.company_id AND statement.bank_account=replacement.bank_account
    WHERE allocation.company_id=invoice.company_id AND replacement.id=allocation.payment_id AND replacement.company_id=invoice.company_id AND replacement.party_id=invoice.party_id AND replacement.currency=invoice.currency
     AND (replacement_created.result_json::jsonb->>'settlementInvoiceId'=invoice.id OR (replacement_created.result_json::jsonb->>'settlementInvoiceId' IS NULL AND EXISTS(
      SELECT 1 FROM command_receipts confirmation WHERE confirmation.operation='finance.payment.confirm' AND confirmation.record_id=replacement.id AND confirmation.actor_id=replacement.released_by
       AND confirmation.authority_json::jsonb->>'companyId'=invoice.company_id AND confirmation.result_json::jsonb->>'id'=replacement.id AND confirmation.result_json::jsonb->>'status'='settled'
       AND confirmation.result_json::jsonb->>'journalId'=replacement.journal_id AND confirmation.result_json::jsonb->>'bankTransactionId'=movement.id
       AND allocation.id='allocation-'||confirmation.command_id||'-0' AND allocation.created_by=confirmation.actor_id)))
     AND replacement.kind='payment' AND replacement.status='settled' AND replacement.outstanding_minor=0 AND replacement.amount_minor=allocation.amount_minor AND replacement.amount_minor>0
     AND replacement.fx_rate_micros=1000000 AND replacement.base_amount_minor=replacement.amount_minor AND COALESCE(replacement.withheld_minor,0)=0
     AND allocation.base_invoice_minor=replacement.amount_minor AND allocation.base_payment_minor=replacement.amount_minor
     AND NOT EXISTS(SELECT 1 FROM finance_allocations other WHERE other.payment_id=replacement.id AND other.id<>allocation.id))))
  AND EXISTS(SELECT 1 FROM command_receipts created WHERE created.operation='finance.supplier_batch.create' AND created.record_id=batch.id AND batch.id='supplier-batch-'||created.command_id AND created.actor_id=batch.created_by AND created.authority_json::jsonb->>'companyId'=batch.company_id AND created.result_json::jsonb->>'id'=batch.id AND created.result_json::jsonb->>'status'='draft')
  AND EXISTS(SELECT 1 FROM command_receipts approved WHERE approved.operation='finance.supplier_batch.approve' AND approved.record_id=batch.id AND approved.actor_id=batch.approved_by AND approved.authority_json::jsonb->>'companyId'=batch.company_id AND approved.result_json::jsonb->>'id'=batch.id AND approved.result_json::jsonb->>'status'='approved')
  AND EXISTS(SELECT 1 FROM command_receipts exported WHERE exported.command_id=batch.export_command_id AND exported.operation='finance.supplier_batch.export' AND exported.record_id=batch.id AND exported.actor_id=batch.exported_by AND exported.authority_json::jsonb->>'companyId'=batch.company_id AND exported.result_json::jsonb->>'id'=batch.id AND exported.result_json::jsonb->>'sha256'=batch.export_hash AND exported.result_json::jsonb->>'rows'=(SELECT count(*)::text FROM finance_supplier_instructions original WHERE original.batch_id=batch.id))
  AND NOT EXISTS(SELECT 1 FROM command_receipts confirmed WHERE confirmed.operation='finance.supplier_batch.confirm' AND confirmed.record_id=failed.id)
  AND NOT EXISTS(SELECT 1 FROM finance_journals original WHERE original.company_id=batch.company_id AND original.source_type='supplier_batch' AND original.source_id=failed.id)`;
