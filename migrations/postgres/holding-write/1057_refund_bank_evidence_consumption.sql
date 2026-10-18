-- New named Refund imports retain a private owner marker in their original
-- finance.bank.import receipt. Every matching writer shares this native table:
-- a marked movement may be consumed only by its exact Refund confirmation.
-- Ordinary imports and original reset/ignored/proposed states stay unchanged.
CREATE FUNCTION finance_assert_refund_bank_consumer(bank_id TEXT, import_statement TEXT)
RETURNS VOID LANGUAGE plpgsql AS $$
DECLARE imported RECORD; movement RECORD; result JSONB; marker JSONB;
BEGIN
 -- The native ID locates one receipt partition; it grants nothing. All actual
 -- receipt/result/Company/owner identities are verified before consumption.
 IF left(import_statement,10)<>'statement-' THEN RETURN; END IF;
 SELECT * INTO imported FROM command_receipts
  WHERE command_id=substring(import_statement FROM 11)
   AND operation='finance.bank.import' AND record_id=import_statement;
 IF NOT FOUND THEN RETURN; END IF;
 result:=imported.result_json::jsonb;
 IF result IS NULL OR NOT (result ? '__refundInstructionBankEvidence') THEN RETURN; END IF;
 marker:=result->'__refundInstructionBankEvidence';
 SELECT movement_row.*,statement.bank_account,statement.imported_by,
  statement.company_id statement_company_id,statement.statement_date
  INTO movement FROM finance_bank_transactions movement_row
  JOIN finance_bank_statements statement ON statement.id=movement_row.statement_id
  WHERE movement_row.id=bank_id;
 IF NOT FOUND THEN
  RAISE EXCEPTION 'REFUND_BANK_PROVENANCE_REQUIRED'
   USING ERRCODE='23514',CONSTRAINT='finance_refund_bank_evidence_consumer';
 END IF;
 -- A still-unmatched scoped movement cannot escape its original receipt by
 -- moving to an ordinary statement before a later matching command.
 IF movement.statement_id IS DISTINCT FROM imported.record_id
  OR bank_id IS DISTINCT FROM 'bankrow-'||imported.command_id||'-0' THEN
  RAISE EXCEPTION 'REFUND_BANK_PROVENANCE_REQUIRED'
   USING ERRCODE='23514',CONSTRAINT='finance_refund_bank_evidence_consumer';
 END IF;
 -- Preserve the original owner lifecycle outside actual matched consumption.
 -- In particular this is not a new generic payment-reversal prohibition.
 IF movement.match_status<>'matched' THEN RETURN; END IF;
 IF marker->>'format' IS DISTINCT FROM 'refund-instruction-bank-evidence/v1'
  OR marker->>'companyId' IS DISTINCT FROM movement.company_id
  OR marker->>'actorId' IS DISTINCT FROM imported.actor_id
  OR imported.authority_json::jsonb->>'companyId' IS DISTINCT FROM movement.company_id
  OR movement.statement_company_id IS DISTINCT FROM movement.company_id
  OR movement.imported_by IS DISTINCT FROM imported.actor_id
  OR result->>'id' IS DISTINCT FROM imported.record_id
  OR result->>'status' IS DISTINCT FROM 'imported'
  OR result->>'imported' IS DISTINCT FROM '1'
  OR result->>'bankTransactionId' IS DISTINCT FROM bank_id
  OR result->>'refundInstructionId' IS DISTINCT FROM marker->>'instructionId'
  OR result->>'refundObligationId' IS DISTINCT FROM marker->>'obligationId'
  OR movement.matched_type IS DISTINCT FROM 'payment'
  OR NOT EXISTS(
   SELECT 1 FROM finance_refund_instructions instruction
   JOIN finance_refund_obligations obligation ON obligation.id=instruction.obligation_id
    AND obligation.company_id=instruction.company_id
   JOIN finance_payments payment ON payment.id=instruction.payment_id
    AND payment.company_id=instruction.company_id AND payment.party_id=obligation.party_id
    AND payment.kind=CASE WHEN obligation.direction='outgoing' THEN 'refund' ELSE 'receipt' END
    AND payment.currency=obligation.currency AND payment.amount_minor=instruction.amount_minor
    AND payment.outstanding_minor=0 AND payment.status='settled'
    AND payment.bank_account=instruction.bank_account
    AND payment.base_amount_minor=instruction.cash_base_minor
    AND payment.fx_journal_id IS NOT DISTINCT FROM instruction.fx_journal_id
   JOIN finance_journals journal ON journal.id=payment.journal_id
    AND journal.company_id=instruction.company_id AND journal.chart_id=obligation.chart_id
    AND journal.currency=obligation.currency AND journal.status='posted'
    AND journal.source_type='return_refund' AND journal.source_id=instruction.id
   JOIN command_receipts confirmation
    ON confirmation.command_id=substring(payment.id FROM 16)
    AND confirmation.operation='finance.refund.confirm'
    AND confirmation.record_id=instruction.id AND confirmation.actor_id=instruction.resolved_by
    AND confirmation.authority_json::jsonb->>'companyId'=instruction.company_id
    AND confirmation.result_json::jsonb->>'id'=instruction.id
    AND confirmation.result_json::jsonb->>'status'='confirmed'
    AND confirmation.result_json::jsonb->>'version'=instruction.version::text
    AND confirmation.result_json::jsonb->>'paymentId'=payment.id
    AND confirmation.result_json::jsonb->>'journalId'=journal.id
    AND confirmation.result_json::jsonb->>'bankTransactionId'=bank_id
    AND confirmation.result_json::jsonb->>'baseCarryingMinor'=instruction.base_carrying_minor::text
    AND confirmation.result_json::jsonb->>'cashBaseMinor'=instruction.cash_base_minor::text
   JOIN money_transaction_snapshots money ON money.company_id=instruction.company_id
    AND money.source_type='finance_payment' AND money.source_id=payment.id
    AND money.amount_kind='settled' AND money.currency=obligation.currency
    AND money.amount_units=instruction.amount_minor
   WHERE instruction.id=marker->>'instructionId'
    AND instruction.obligation_id=marker->>'obligationId'
    AND instruction.company_id=movement.company_id
    AND instruction.status='confirmed' AND instruction.bank_transaction_id=bank_id
    AND instruction.payment_id=movement.matched_id
    AND instruction.bank_account=movement.bank_account
    AND instruction.cash_account_id=movement.bank_account
    AND movement.currency=obligation.currency
    AND movement.amount_minor=CASE WHEN obligation.direction='outgoing'
      THEN -instruction.amount_minor ELSE instruction.amount_minor END
    AND movement.transaction_date>=instruction.planned_date
    AND movement.transaction_date<=movement.statement_date
    AND payment.id='refund-payment-'||confirmation.command_id
    AND payment.created_by=confirmation.actor_id
  ) THEN
  RAISE EXCEPTION 'REFUND_BANK_PROVENANCE_REQUIRED'
   USING ERRCODE='23514',CONSTRAINT='finance_refund_bank_evidence_consumer';
 END IF;
END;
$$;

CREATE FUNCTION guard_finance_refund_bank_consumer() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='UPDATE' THEN
  PERFORM finance_assert_refund_bank_consumer(OLD.id,OLD.statement_id);
  IF (NEW.id,NEW.statement_id) IS DISTINCT FROM (OLD.id,OLD.statement_id) THEN
   PERFORM finance_assert_refund_bank_consumer(NEW.id,NEW.statement_id);
  END IF;
 ELSE
  PERFORM finance_assert_refund_bank_consumer(NEW.id,NEW.statement_id);
 END IF;
 RETURN NULL;
END;
$$;
-- The original command writes its receipt, instruction, payment and journal
-- atomically. Check their complete final state after original deferred effects.
CREATE CONSTRAINT TRIGGER finance_refund_bank_evidence_consumer
 AFTER INSERT OR UPDATE ON finance_bank_transactions DEFERRABLE INITIALLY DEFERRED
 FOR EACH ROW EXECUTE FUNCTION guard_finance_refund_bank_consumer();

-- CREATE TRIGGER does not validate existing movements. Reject any previously
-- misconsumed marked evidence in this same atomic migration; do not grandfather
-- the demonstrated bug or rewrite its payment/history. Unmarked imports no-op.
DO $$ DECLARE imported RECORD; BEGIN
 FOR imported IN SELECT record_id,result_json::jsonb->>'bankTransactionId' bank_id
  FROM command_receipts WHERE operation='finance.bank.import'
   AND result_json::jsonb ? '__refundInstructionBankEvidence'
 LOOP
  PERFORM finance_assert_refund_bank_consumer(imported.bank_id,imported.record_id);
 END LOOP;
END $$;
