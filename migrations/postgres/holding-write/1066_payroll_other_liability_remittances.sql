-- Add lawful settlement for the three nonstatutory liability descriptions
-- emitted by the original Payroll accrual. Original statutory rows/receipts
-- remain unchanged; no rate, assignment, account or money is manufactured.
ALTER TABLE hr_payroll_statutory_remittances DROP CONSTRAINT hr_payroll_statutory_remittances_kind_check;
ALTER TABLE hr_payroll_statutory_remittances ADD CONSTRAINT hr_payroll_statutory_remittances_kind_check
 CHECK(kind IN ('withholding_tax','social_security','benefit','deduction_other','employer_other'));
ALTER TABLE hr_payroll_statutory_remittances ADD COLUMN reason TEXT;
ALTER TABLE hr_payroll_statutory_remittances ADD CONSTRAINT payroll_other_remittance_reason
 CHECK(kind IN ('withholding_tax','social_security') OR (reason IS NOT NULL AND length(btrim(reason)) BETWEEN 3 AND 3000));

CREATE FUNCTION payroll_assert_other_remittance(remittance_id TEXT) RETURNS VOID LANGUAGE plpgsql AS $$
DECLARE r RECORD;
BEGIN
 SELECT * INTO r FROM hr_payroll_statutory_remittances WHERE id=remittance_id;
 IF NOT FOUND THEN RAISE EXCEPTION 'PAYROLL_REMITTANCE_SOURCE_CHANGED' USING ERRCODE='23514'; END IF;
 IF r.kind IN ('withholding_tax','social_security') THEN RETURN; END IF;
 IF NOT EXISTS(
  SELECT 1 FROM hr_payroll_runs run JOIN hr_payroll_postings source ON source.run_id=run.id
  JOIN finance_journals accrual ON accrual.id=source.journal_id AND accrual.company_id=run.company_id
   AND accrual.status='posted' AND accrual.source_type='payroll' AND accrual.source_id=run.id
  JOIN command_receipts original ON original.operation='people.payroll.post' AND original.record_id=run.id
   AND original.actor_id=run.posted_by AND original.authority_json::jsonb->>'companyId'=run.company_id
   AND original.result_json::jsonb->>'id'=run.id AND original.result_json::jsonb->>'status'='posted'
   AND original.result_json::jsonb->>'journalId'=accrual.id
  JOIN finance_journals journal ON journal.id=r.journal_id AND journal.company_id=r.company_id
   AND journal.status='posted' AND journal.chart_id=accrual.chart_id AND journal.currency=run.currency
   AND journal.source_type='payroll_remittance' AND journal.source_id=r.id AND journal.posted_by=r.recorded_by
  JOIN finance_bank_transactions bank ON bank.id=r.bank_transaction_id AND bank.company_id=r.company_id
   AND bank.match_status='matched' AND bank.matched_type='payroll_remittance' AND bank.matched_id=r.id
   AND bank.amount_minor=-r.amount_minor AND bank.currency=run.currency AND bank.external_id=r.bank_reference
   AND bank.transaction_date>=run.period_end AND journal.entry_date=bank.transaction_date
  JOIN finance_bank_statements statement ON statement.id=bank.statement_id AND statement.company_id=r.company_id
   AND statement.bank_account=r.bank_account_id
  JOIN command_receipts bank_import ON bank_import.operation='finance.bank.import' AND bank_import.record_id=statement.id
   AND statement.id='statement-'||bank_import.command_id AND bank.id='bankrow-'||bank_import.command_id||'-0'
   AND bank_import.authority_json::jsonb->>'companyId'=r.company_id
   AND bank_import.result_json::jsonb->>'bankTransactionId'=bank.id
   AND bank_import.result_json::jsonb->'__payrollOtherLiabilityBankEvidence'->>'runId'=r.run_id
   AND bank_import.result_json::jsonb->'__payrollOtherLiabilityBankEvidence'->>'kind'=r.kind
   AND bank_import.result_json::jsonb->'__payrollOtherLiabilityBankEvidence'->>'liabilityAccountId'=r.liability_account_id
  JOIN command_receipts command ON r.id='payroll-remit-'||command.command_id
   AND command.operation='payroll.liability.remit' AND command.record_id=run.id AND command.actor_id=r.recorded_by
   AND command.authority_json::jsonb->>'companyId'=r.company_id
   AND command.result_json::jsonb->>'id'=r.id AND command.result_json::jsonb->>'runId'=run.id
   AND command.result_json::jsonb->>'kind'=r.kind
   AND command.result_json::jsonb->>'liabilityAccountId'=r.liability_account_id
   AND command.result_json::jsonb->>'amountMinor'=r.amount_minor::text
   AND command.result_json::jsonb->>'journalId'=journal.id
  JOIN money_transaction_snapshots money ON money.company_id=r.company_id
   AND money.source_type='payroll_other_remittance' AND money.source_id=r.id AND money.amount_kind='paid'
   AND money.currency=run.currency AND money.currency_scale=2 AND money.amount_units=r.amount_minor
   AND money.transaction_date=bank.transaction_date::date
  WHERE run.id=r.run_id AND run.company_id=r.company_id AND run.status='posted'
   AND EXISTS(SELECT 1 FROM finance_journal_lines line WHERE line.journal_id=accrual.id
    AND line.account_id=r.liability_account_id AND line.description=CASE r.kind WHEN 'benefit' THEN 'Payroll benefit' WHEN 'deduction_other' THEN 'Payroll deduction other' WHEN 'employer_other' THEN 'Payroll employer other' END
    GROUP BY line.account_id HAVING SUM(line.credit_minor-line.debit_minor)>0)
   AND (SELECT SUM(line.credit_minor-line.debit_minor) FROM finance_journal_lines line WHERE line.journal_id=accrual.id AND line.account_id=r.liability_account_id
    AND line.description=CASE r.kind WHEN 'benefit' THEN 'Payroll benefit' WHEN 'deduction_other' THEN 'Payroll deduction other' WHEN 'employer_other' THEN 'Payroll employer other' END)
    >=(SELECT SUM(other.amount_minor) FROM hr_payroll_statutory_remittances other WHERE other.run_id=r.run_id AND other.company_id=r.company_id AND other.kind=r.kind AND other.liability_account_id=r.liability_account_id)
   AND (SELECT COUNT(*) FROM finance_journal_lines line WHERE line.journal_id=journal.id)=2
   AND (SELECT SUM(line.debit_minor) FROM finance_journal_lines line WHERE line.journal_id=journal.id AND line.account_id=r.liability_account_id)=r.amount_minor
   AND (SELECT SUM(line.credit_minor) FROM finance_journal_lines line WHERE line.journal_id=journal.id AND line.account_id=r.bank_account_id)=r.amount_minor
 ) THEN RAISE EXCEPTION 'PAYROLL_REMITTANCE_SOURCE_CHANGED' USING ERRCODE='23514'; END IF;
END $$;

CREATE FUNCTION finance_assert_payroll_other_bank(bank_id TEXT, import_statement TEXT) RETURNS VOID LANGUAGE plpgsql AS $$
DECLARE receipt RECORD; movement RECORD; marker JSONB; result JSONB; ignore_key TEXT; ignore_count INTEGER;
BEGIN
 IF left(import_statement,10)<>'statement-' THEN RETURN; END IF;
 SELECT * INTO receipt FROM command_receipts WHERE command_id=substring(import_statement FROM 11)
  AND operation='finance.bank.import' AND record_id=import_statement;
 IF NOT FOUND THEN RETURN; END IF;
 result:=receipt.result_json::jsonb;IF result IS NULL OR NOT jsonb_exists(result,'__payrollOtherLiabilityBankEvidence') THEN RETURN; END IF;
 marker:=result->'__payrollOtherLiabilityBankEvidence';
 SELECT bank.*,statement.company_id statement_company,statement.bank_account,statement.imported_by,statement.statement_date,statement.statement_number,statement.opening_minor,statement.closing_minor,statement.closing_fx_rate_micros,statement.base_closing_minor,statement.source_hash,statement.created_at statement_created_at INTO movement
  FROM finance_bank_transactions bank JOIN finance_bank_statements statement ON statement.id=bank.statement_id WHERE bank.id=bank_id;
 IF NOT FOUND THEN RAISE EXCEPTION 'PAYROLL_BANK_PROVENANCE_REQUIRED' USING ERRCODE='23514'; END IF;
 IF movement.id IS DISTINCT FROM 'bankrow-'||receipt.command_id||'-0' OR movement.statement_id IS DISTINCT FROM receipt.record_id
  OR marker->>'format' IS DISTINCT FROM 'payroll-other-liability-bank-evidence/v1'
  OR marker->>'companyId' IS DISTINCT FROM movement.company_id OR marker->>'actorId' IS DISTINCT FROM receipt.actor_id
  OR receipt.authority_json::jsonb->>'companyId' IS DISTINCT FROM movement.company_id
  OR movement.statement_company IS DISTINCT FROM movement.company_id OR movement.imported_by IS DISTINCT FROM receipt.actor_id
  OR result->>'id' IS DISTINCT FROM import_statement OR result->>'status' IS DISTINCT FROM 'imported' OR result->>'imported' IS DISTINCT FROM '1'
  OR result->>'bankTransactionId' IS DISTINCT FROM movement.id OR result->>'payrollRunId' IS DISTINCT FROM marker->>'runId'
  OR result->>'kind' IS DISTINCT FROM marker->>'kind' OR result->>'liabilityAccountId' IS DISTINCT FROM marker->>'liabilityAccountId'
  OR marker->>'bankAccountId' IS DISTINCT FROM movement.bank_account OR marker->>'currency' IS DISTINCT FROM movement.currency
  OR marker->>'amountMinor' IS DISTINCT FROM movement.amount_minor::text OR movement.amount_minor>=0
  OR marker->>'externalId' IS DISTINCT FROM movement.external_id OR marker->>'transactionDate' IS DISTINCT FROM movement.transaction_date::text
  OR marker->'movementImage' IS DISTINCT FROM jsonb_build_array(movement.id,movement.company_id,movement.statement_id,movement.transaction_date,movement.external_id,movement.description,movement.amount_minor,movement.currency)
  OR marker->'statementImage' IS DISTINCT FROM jsonb_build_array(movement.statement_id,movement.statement_company,movement.bank_account,movement.statement_number,movement.statement_date,movement.opening_minor,movement.closing_minor,movement.closing_fx_rate_micros,movement.base_closing_minor,movement.source_hash,movement.imported_by,movement.statement_created_at)
  OR movement.transaction_date>movement.statement_date OR (SELECT COUNT(*) FROM finance_bank_transactions bank WHERE bank.statement_id=import_statement)<>1
  OR NOT EXISTS(SELECT 1 FROM hr_payroll_runs run JOIN hr_payroll_postings posting ON posting.run_id=run.id
   JOIN finance_journals journal ON journal.id=posting.journal_id AND journal.company_id=run.company_id AND journal.status='posted' AND journal.source_type='payroll' AND journal.source_id=run.id
   JOIN command_receipts original ON original.command_id=marker->>'postingCommandId' AND original.operation='people.payroll.post'
    AND original.record_id=run.id AND original.actor_id=run.posted_by AND original.authority_json::jsonb->>'companyId'=run.company_id
    AND original.result_json::jsonb->>'journalId'=journal.id AND original.result_json::jsonb->>'status'='posted'
   WHERE run.id=marker->>'runId' AND run.company_id=movement.company_id AND run.status='posted' AND run.currency=movement.currency
    AND movement.transaction_date>=run.period_end
    AND (SELECT SUM(line.credit_minor-line.debit_minor) FROM finance_journal_lines line WHERE line.journal_id=journal.id
     AND line.account_id=marker->>'liabilityAccountId' AND line.description=CASE marker->>'kind' WHEN 'benefit' THEN 'Payroll benefit' WHEN 'deduction_other' THEN 'Payroll deduction other' WHEN 'employer_other' THEN 'Payroll employer other' END)>=-movement.amount_minor)
 THEN RAISE EXCEPTION 'PAYROLL_BANK_PROVENANCE_REQUIRED' USING ERRCODE='23514'; END IF;
 -- An ignored source observation retains its actual owner decision forever;
 -- a stale generic reset cannot silently reactivate it as a new obligation.
 SELECT COUNT(*),MIN(command_id) INTO ignore_count,ignore_key FROM command_receipts
  WHERE operation='payroll.liability.bank-evidence.ignore' AND record_id=movement.id;
 IF movement.match_status='ignored' OR ignore_count>0 THEN
  IF ignore_count<>1 THEN RAISE EXCEPTION 'PAYROLL_BANK_PROVENANCE_REQUIRED' USING ERRCODE='23514'; END IF;
  PERFORM payroll_assert_other_bank_ignore(ignore_key);
 END IF;
 -- Original unmatched/proposed states remain available. Actual matched
 -- consumption must prove the exact original Payroll remittance/payment.
 IF movement.match_status='matched' THEN
  IF movement.matched_type IS DISTINCT FROM 'payroll_remittance' OR NOT EXISTS(SELECT 1 FROM hr_payroll_statutory_remittances remittance WHERE remittance.id=movement.matched_id AND remittance.bank_transaction_id=movement.id AND remittance.company_id=movement.company_id AND remittance.run_id=marker->>'runId' AND remittance.kind=marker->>'kind' AND remittance.liability_account_id=marker->>'liabilityAccountId') THEN RAISE EXCEPTION 'PAYROLL_BANK_PROVENANCE_REQUIRED' USING ERRCODE='23514'; END IF;
  PERFORM payroll_assert_other_remittance(movement.matched_id);
 END IF;
END $$;

CREATE FUNCTION payroll_assert_other_bank_ignore(ignore_key TEXT) RETURNS VOID LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM command_receipts decision
  JOIN finance_bank_transactions bank ON bank.id=decision.record_id AND bank.match_status='ignored' AND bank.matched_type IS NULL AND bank.matched_id IS NULL
  JOIN finance_bank_statements statement ON statement.id=bank.statement_id AND statement.company_id=bank.company_id
  JOIN command_receipts original ON original.operation='finance.bank.import' AND original.record_id=statement.id
   AND statement.id='statement-'||original.command_id AND bank.id='bankrow-'||original.command_id||'-0'
  JOIN audit_events audit ON audit.command_id=decision.command_id AND audit.action=decision.operation AND audit.actor_id=decision.actor_id AND audit.record_id=bank.id
  WHERE decision.command_id=ignore_key AND decision.operation='payroll.liability.bank-evidence.ignore'
   AND decision.authority_json::jsonb->>'companyId'=bank.company_id
   AND decision.result_json::jsonb->>'id'=bank.id AND decision.result_json::jsonb->>'status'='ignored'
   AND decision.result_json::jsonb->>'version'~'^[1-9][0-9]*$' AND bank.version>=(decision.result_json::jsonb->>'version')::bigint AND audit.record_version=(decision.result_json::jsonb->>'version')::bigint
   AND decision.result_json::jsonb->'__payrollOtherLiabilityBankIgnore'->>'format'='payroll-other-liability-bank-ignore/v1'
   AND decision.result_json::jsonb->'__payrollOtherLiabilityBankIgnore'->>'commandId'=decision.command_id
   AND decision.result_json::jsonb->'__payrollOtherLiabilityBankIgnore'->>'actorId'=decision.actor_id
   AND decision.result_json::jsonb->'__payrollOtherLiabilityBankIgnore'->>'importCommandId'=original.command_id
   AND decision.result_json::jsonb->'__payrollOtherLiabilityBankIgnore'->>'bankVersion'~'^[1-9][0-9]*$'
   AND (decision.result_json::jsonb->'__payrollOtherLiabilityBankIgnore'->>'bankVersion')::bigint+1=(decision.result_json::jsonb->>'version')::bigint
   AND length(btrim(decision.result_json::jsonb->'__payrollOtherLiabilityBankIgnore'->>'reason')) BETWEEN 3 AND 3000
   AND decision.result_json::jsonb->>'payrollRunId'=original.result_json::jsonb->'__payrollOtherLiabilityBankEvidence'->>'runId'
   AND decision.result_json::jsonb->>'kind'=original.result_json::jsonb->'__payrollOtherLiabilityBankEvidence'->>'kind'
   AND decision.result_json::jsonb->>'liabilityAccountId'=original.result_json::jsonb->'__payrollOtherLiabilityBankEvidence'->>'liabilityAccountId'
   AND decision.result_json::jsonb->'__payrollOtherLiabilityBankIgnore'->>'sourceFingerprint'=original.result_json::jsonb->'__payrollOtherLiabilityBankEvidence'->>'sourceFingerprint'
   AND decision.result_json::jsonb->'__payrollOtherLiabilityBankIgnore'->>'runId'=decision.result_json::jsonb->>'payrollRunId'
   AND decision.result_json::jsonb->'__payrollOtherLiabilityBankIgnore'->>'kind'=decision.result_json::jsonb->>'kind'
   AND decision.result_json::jsonb->'__payrollOtherLiabilityBankIgnore'->>'liabilityAccountId'=decision.result_json::jsonb->>'liabilityAccountId'
 ) THEN RAISE EXCEPTION 'PAYROLL_BANK_PROVENANCE_REQUIRED' USING ERRCODE='23514'; END IF;
END $$;

CREATE FUNCTION guard_payroll_other_sources() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
 IF TG_ARGV[0]='receipt' THEN
  IF NEW.operation='finance.bank.import' AND jsonb_exists(NEW.result_json::jsonb,'__payrollOtherLiabilityBankEvidence') THEN PERFORM finance_assert_payroll_other_bank(NEW.result_json::jsonb->>'bankTransactionId',NEW.record_id);
  ELSIF NEW.operation='payroll.liability.bank-evidence.ignore' THEN PERFORM payroll_assert_other_bank_ignore(NEW.command_id);
  ELSIF NEW.operation='payroll.liability.remit' AND NEW.result_json::jsonb->>'kind' IN ('benefit','deduction_other','employer_other') THEN PERFORM payroll_assert_other_remittance(NEW.result_json::jsonb->>'id'); END IF;
 ELSIF TG_ARGV[0]='statement' THEN
  PERFORM finance_assert_payroll_other_bank('bankrow-'||substring(OLD.id FROM 11)||'-0',OLD.id);
 ELSIF TG_ARGV[0]='bank' THEN
  IF TG_OP='DELETE' THEN PERFORM finance_assert_payroll_other_bank(OLD.id,OLD.statement_id); RETURN NULL; END IF;
  IF TG_OP='UPDATE' THEN PERFORM finance_assert_payroll_other_bank(OLD.id,OLD.statement_id); END IF;
  PERFORM finance_assert_payroll_other_bank(NEW.id,NEW.statement_id);
 ELSE
  IF NEW.kind IN ('benefit','deduction_other','employer_other') THEN PERFORM payroll_assert_other_remittance(NEW.id); END IF;
 END IF;
 RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER payroll_other_remittance_source AFTER INSERT OR UPDATE ON hr_payroll_statutory_remittances DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION guard_payroll_other_sources('remittance');
CREATE CONSTRAINT TRIGGER payroll_other_bank_source AFTER INSERT OR UPDATE OR DELETE ON finance_bank_transactions DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION guard_payroll_other_sources('bank');
CREATE CONSTRAINT TRIGGER payroll_other_statement_source AFTER UPDATE OR DELETE ON finance_bank_statements DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION guard_payroll_other_sources('statement');
CREATE CONSTRAINT TRIGGER payroll_other_receipt_source AFTER INSERT OR UPDATE ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION guard_payroll_other_sources('receipt');

CREATE FUNCTION protect_payroll_other_birth() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
 IF TG_ARGV[0]='receipt' THEN
  -- Native receipt authority capture may update authority_json. An existing
  -- private source receipt cannot be erased, reclassified or moved instead.
  IF TG_OP='DELETE' THEN
   IF (OLD.operation='finance.bank.import' AND jsonb_exists(OLD.result_json::jsonb,'__payrollOtherLiabilityBankEvidence')) OR OLD.operation='payroll.liability.bank-evidence.ignore' OR (OLD.operation='payroll.liability.remit' AND OLD.result_json::jsonb->>'kind' IN ('benefit','deduction_other','employer_other')) THEN RAISE EXCEPTION 'PAYROLL_REMITTANCE_SOURCE_CHANGED' USING ERRCODE='23514'; END IF;
   RETURN OLD;
  END IF;
  IF OLD.operation='finance.bank.import' OR NEW.operation='finance.bank.import' THEN
   IF (OLD.result_json::jsonb->'__payrollOtherLiabilityBankEvidence') IS DISTINCT FROM (NEW.result_json::jsonb->'__payrollOtherLiabilityBankEvidence') OR (OLD.operation<>'finance.bank.import' AND jsonb_exists(NEW.result_json::jsonb,'__payrollOtherLiabilityBankEvidence')) OR (jsonb_exists(OLD.result_json::jsonb,'__payrollOtherLiabilityBankEvidence') AND (OLD.command_id,OLD.operation,OLD.record_id,OLD.actor_id,OLD.request_hash,OLD.result_json) IS DISTINCT FROM (NEW.command_id,NEW.operation,NEW.record_id,NEW.actor_id,NEW.request_hash,NEW.result_json)) THEN RAISE EXCEPTION 'PAYROLL_REMITTANCE_SOURCE_CHANGED' USING ERRCODE='23514'; END IF;
  END IF;
  IF (OLD.operation='payroll.liability.bank-evidence.ignore' OR NEW.operation='payroll.liability.bank-evidence.ignore') AND (OLD.command_id,OLD.operation,OLD.record_id,OLD.actor_id,OLD.request_hash,OLD.result_json) IS DISTINCT FROM (NEW.command_id,NEW.operation,NEW.record_id,NEW.actor_id,NEW.request_hash,NEW.result_json) THEN RAISE EXCEPTION 'PAYROLL_REMITTANCE_SOURCE_CHANGED' USING ERRCODE='23514'; END IF;
  IF ((OLD.operation='payroll.liability.remit' AND OLD.result_json::jsonb->>'kind' IN ('benefit','deduction_other','employer_other')) OR (NEW.operation='payroll.liability.remit' AND NEW.result_json::jsonb->>'kind' IN ('benefit','deduction_other','employer_other'))) AND (OLD.command_id,OLD.operation,OLD.record_id,OLD.actor_id,OLD.request_hash,OLD.result_json) IS DISTINCT FROM (NEW.command_id,NEW.operation,NEW.record_id,NEW.actor_id,NEW.request_hash,NEW.result_json) THEN RAISE EXCEPTION 'PAYROLL_REMITTANCE_SOURCE_CHANGED' USING ERRCODE='23514'; END IF;
  RETURN NEW;
 END IF;
 IF OLD.kind IN ('benefit','deduction_other','employer_other') THEN
  IF TG_OP='DELETE' THEN RAISE EXCEPTION 'PAYROLL_REMITTANCE_SOURCE_CHANGED' USING ERRCODE='23514'; END IF;
  IF to_jsonb(OLD) IS DISTINCT FROM to_jsonb(NEW) THEN RAISE EXCEPTION 'PAYROLL_REMITTANCE_SOURCE_CHANGED' USING ERRCODE='23514'; END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER payroll_other_receipt_birth BEFORE UPDATE OR DELETE ON command_receipts FOR EACH ROW EXECUTE FUNCTION protect_payroll_other_birth('receipt');
CREATE TRIGGER payroll_other_remittance_immutable BEFORE UPDATE OR DELETE ON hr_payroll_statutory_remittances FOR EACH ROW EXECUTE FUNCTION protect_payroll_other_birth('remittance');
-- Reject invalid pre-existing private markers atomically; never grandfather or
-- repair an already consumed/forged source while installing this additive SQL.
DO $$ DECLARE receipt RECORD; BEGIN
 FOR receipt IN SELECT record_id,result_json::jsonb->>'bankTransactionId' bank_id FROM command_receipts WHERE operation='finance.bank.import' AND jsonb_exists(result_json::jsonb,'__payrollOtherLiabilityBankEvidence') LOOP PERFORM finance_assert_payroll_other_bank(receipt.bank_id,receipt.record_id); END LOOP;
END $$;
