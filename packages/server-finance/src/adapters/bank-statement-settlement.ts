import 'server-only';
import {supplierPaymentOriginalSources} from '@erpx/server-finance/adapters/supplier-payment-correction-sources';

/** Fixed movement and statement aliases. Supplier confirmation has its own
 * receipt and immutable posting capture, not generic payment money snapshots. */
const supplierBatchMovementSettled=`EXISTS(SELECT 1 FROM (${supplierPaymentOriginalSources}) supplier_source
 JOIN finance_payments payment ON payment.id=supplier_source.id
 JOIN finance_supplier_instructions instruction ON instruction.id=supplier_source.instruction_id
 JOIN finance_supplier_batches batch ON batch.id=supplier_source.batch_id
 JOIN finance_journals journal ON journal.id=payment.journal_id AND journal.status='posted'
 JOIN finance_financial_events captured ON captured.journal_id=journal.id AND captured.company_id=journal.company_id AND captured.chart_id=journal.chart_id AND captured.period_id=journal.period_id AND captured.currency=journal.currency AND captured.base_currency=payment.currency AND captured.source_type='supplier_batch' AND captured.source_id=instruction.id AND captured.effective_date=journal.entry_date AND captured.legacy_snapshot=0
 JOIN command_receipts confirmed ON confirmed.operation='finance.supplier_batch.confirm' AND confirmed.record_id=instruction.id AND confirmed.actor_id=instruction.resolved_by AND confirmed.authority_json::jsonb->>'companyId'=payment.company_id AND confirmed.result_json::jsonb->>'id'=instruction.id AND confirmed.result_json::jsonb->>'status'='confirmed' AND confirmed.result_json::jsonb->>'paymentId'=payment.id AND confirmed.result_json::jsonb->>'journalId'=journal.id AND payment.id='supplier-payment-'||confirmed.command_id
 WHERE supplier_source.id=movement.matched_id AND supplier_source.company_id=r.company_id AND supplier_source.bank_transaction_id=movement.id AND supplier_source.bank_account=r.bank_account AND supplier_source.currency=movement.currency
  AND payment.status='settled' AND payment.outstanding_minor=0 AND payment.fx_rate_micros=1000000 AND payment.base_amount_minor=payment.amount_minor AND payment.withheld_minor=0
  AND movement.amount_minor=-payment.amount_minor AND payment.payment_date=movement.transaction_date AND movement.transaction_date>=batch.planned_date AND journal.entry_date=movement.transaction_date
  AND batch.created_by<>batch.approved_by AND batch.created_by<>batch.exported_by AND batch.approved_by<>batch.exported_by
  AND EXISTS(SELECT 1 FROM command_receipts created WHERE created.operation='finance.supplier_batch.create' AND created.record_id=batch.id AND batch.id='supplier-batch-'||created.command_id AND created.actor_id=batch.created_by AND created.authority_json::jsonb->>'companyId'=batch.company_id AND created.result_json::jsonb->>'id'=batch.id AND created.result_json::jsonb->>'status'='draft')
  AND EXISTS(SELECT 1 FROM command_receipts approved WHERE approved.operation='finance.supplier_batch.approve' AND approved.record_id=batch.id AND approved.actor_id=batch.approved_by AND approved.authority_json::jsonb->>'companyId'=batch.company_id AND approved.result_json::jsonb->>'id'=batch.id AND approved.result_json::jsonb->>'status'='approved')
  AND EXISTS(SELECT 1 FROM command_receipts exported WHERE exported.command_id=batch.export_command_id AND exported.operation='finance.supplier_batch.export' AND exported.record_id=batch.id AND exported.actor_id=batch.exported_by AND exported.authority_json::jsonb->>'companyId'=batch.company_id AND exported.result_json::jsonb->>'id'=batch.id AND exported.result_json::jsonb->>'sha256'=batch.export_hash AND exported.result_json::jsonb->>'rows'=(SELECT count(*)::text FROM finance_supplier_instructions item WHERE item.batch_id=batch.id))
  AND captured.evidence_json::jsonb->'journal'->>'id'=journal.id AND captured.evidence_json::jsonb->'journal'->>'status'='posted'
  AND captured.evidence_json::jsonb->'journal'->>'source_type'=journal.source_type AND captured.evidence_json::jsonb->'journal'->>'source_id'=journal.source_id AND captured.evidence_json::jsonb->'journal'->>'company_id'=journal.company_id AND captured.evidence_json::jsonb->'journal'->>'chart_id'=journal.chart_id AND captured.evidence_json::jsonb->'journal'->>'period_id'=journal.period_id AND captured.evidence_json::jsonb->'journal'->>'currency'=journal.currency AND captured.evidence_json::jsonb->'journal'->>'entry_date'=journal.entry_date
  AND captured.evidence_json::jsonb->'lines'=(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.ordinal) FROM finance_journal_lines line WHERE line.journal_id=journal.id)
  AND (SELECT count(*)>=2 AND sum(line.debit_minor)=payment.amount_minor AND sum(line.base_debit_minor)=payment.amount_minor AND sum(line.debit_minor-line.credit_minor)=0 AND sum(line.base_debit_minor-line.base_credit_minor)=0 FROM finance_journal_lines line WHERE line.journal_id=journal.id)
  AND (SELECT sum(line.debit_minor-line.credit_minor) FROM finance_journal_lines line WHERE line.journal_id=journal.id AND line.account_id=r.bank_account)=movement.amount_minor
 )`;

/** Fixed statement alias r. The original import retains status=imported;
 * independently confirmed Finance payments or supplier instructions provide
 * its actual settlement through their separate original owner receipts.
 * Other matchers and incomplete/ambiguous imports remain conservative duties. */
export const controlledPaymentStatementSettled=`COALESCE((
 r.status IN ('imported','matching')
 AND EXISTS(SELECT 1 FROM finance_accounts bank_account JOIN finance_charts bank_chart ON bank_chart.id=bank_account.chart_id AND bank_chart.company_id=bank_account.company_id WHERE bank_account.id=r.bank_account AND bank_account.company_id=r.company_id AND bank_account.category='asset' AND bank_account.control_kind='bank')
 AND EXISTS(SELECT 1 FROM command_receipts imported WHERE imported.operation='finance.bank.import' AND imported.record_id=r.id AND r.id='statement-'||imported.command_id AND imported.actor_id=r.imported_by AND imported.authority_json::jsonb->>'companyId'=r.company_id AND imported.result_json::jsonb->>'id'=r.id AND imported.result_json::jsonb->>'status'='imported'
  AND imported.result_json::jsonb->>'imported'=(SELECT count(*)::text FROM finance_bank_transactions movement WHERE movement.statement_id=r.id)
  AND NOT EXISTS(SELECT 1 FROM finance_bank_transactions movement WHERE movement.statement_id=r.id AND (movement.id NOT LIKE 'bankrow-'||imported.command_id||'-%' OR movement.company_id<>r.company_id)))
 AND EXISTS(SELECT 1 FROM finance_bank_transactions movement WHERE movement.statement_id=r.id)
 AND r.opening_minor+(SELECT sum(movement.amount_minor) FROM finance_bank_transactions movement WHERE movement.statement_id=r.id)=r.closing_minor
 AND NOT EXISTS(SELECT 1 FROM finance_bank_transactions movement WHERE movement.statement_id=r.id AND NOT COALESCE((
  movement.company_id=r.company_id AND movement.transaction_date<=r.statement_date AND movement.match_status='matched' AND movement.matched_type='payment'
  AND (EXISTS(SELECT 1 FROM finance_payments payment
   JOIN finance_accounts bank_account ON bank_account.id=payment.bank_account AND bank_account.company_id=payment.company_id AND bank_account.currency=payment.currency
   JOIN finance_journals journal ON journal.id=payment.journal_id AND journal.company_id=payment.company_id AND journal.chart_id=bank_account.chart_id AND journal.currency=payment.currency AND journal.source_type='payment' AND journal.source_id=payment.id AND journal.status='posted'
   JOIN command_receipts confirmed ON confirmed.operation='finance.payment.confirm' AND confirmed.record_id=payment.id AND confirmed.actor_id=payment.released_by AND confirmed.authority_json::jsonb->>'companyId'=payment.company_id AND confirmed.result_json::jsonb->>'id'=payment.id AND confirmed.result_json::jsonb->>'status'='settled' AND confirmed.result_json::jsonb->>'journalId'=journal.id AND confirmed.result_json::jsonb->>'bankTransactionId'=movement.id
   WHERE payment.id=movement.matched_id AND payment.company_id=r.company_id AND payment.bank_account=r.bank_account AND payment.currency=movement.currency AND payment.kind IN ('payment','receipt') AND payment.status='settled' AND payment.outstanding_minor=0 AND payment.amount_minor>0
    AND payment.created_by<>payment.approved_by AND payment.created_by<>payment.released_by AND payment.approved_by<>payment.released_by
    AND movement.transaction_date>=payment.payment_date AND movement.amount_minor=CASE WHEN payment.kind='receipt' THEN payment.amount_minor ELSE -(payment.amount_minor-COALESCE(payment.withheld_minor,0)) END
    AND EXISTS(SELECT 1 FROM money_transaction_snapshots money WHERE money.company_id=payment.company_id AND money.source_type='finance_payment' AND money.source_id=payment.id AND money.amount_kind='settled' AND money.currency=payment.currency AND money.amount_units=payment.amount_minor)
    AND EXISTS(SELECT 1 FROM money_transaction_snapshots money WHERE money.company_id=payment.company_id AND money.source_type='finance_payment' AND money.source_id=payment.id AND money.amount_kind='bank_settled' AND money.currency=payment.currency AND money.amount_units=payment.amount_minor-COALESCE(payment.withheld_minor,0))
    AND (SELECT count(*)>=2 AND sum(line.debit_minor)=payment.amount_minor AND sum(line.debit_minor-line.credit_minor)=0 AND sum(line.base_debit_minor-line.base_credit_minor)=0 FROM finance_journal_lines line WHERE line.journal_id=journal.id)
    AND (SELECT sum(line.debit_minor-line.credit_minor) FROM finance_journal_lines line WHERE line.journal_id=journal.id AND line.account_id=r.bank_account)=movement.amount_minor
  ) OR ${supplierBatchMovementSettled})
 ),FALSE))
),FALSE)`;
