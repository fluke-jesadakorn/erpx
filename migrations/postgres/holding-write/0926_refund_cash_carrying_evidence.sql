-- New confirmations retain their carried source control value separately from
-- the bank-date cash value. No historical foreign quote is invented.
ALTER TABLE finance_refund_instructions ADD COLUMN fx_gain_account_id TEXT REFERENCES finance_accounts(id);
ALTER TABLE finance_refund_instructions ADD COLUMN fx_loss_account_id TEXT REFERENCES finance_accounts(id);
ALTER TABLE finance_refund_instructions ADD COLUMN base_carrying_minor BIGINT CHECK(base_carrying_minor>=0);
ALTER TABLE finance_refund_instructions ADD COLUMN cash_base_minor BIGINT CHECK(cash_base_minor>=0);
ALTER TABLE finance_refund_instructions ADD COLUMN fx_journal_id TEXT REFERENCES finance_journals(id);
UPDATE finance_refund_instructions i SET base_carrying_minor=i.amount_minor,cash_base_minor=i.amount_minor
FROM finance_refund_obligations o JOIN finance_charts c ON c.id=o.chart_id
WHERE i.obligation_id=o.id AND i.status='confirmed' AND o.currency=c.base_currency;
CREATE FUNCTION finance_refund_instruction_evidence_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' THEN RAISE EXCEPTION 'REFUND_INSTRUCTION_EVIDENCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF (NEW.obligation_id,NEW.company_id,NEW.amount_minor,NEW.planned_date,NEW.bank_account,NEW.cash_account_id,NEW.fx_gain_account_id,NEW.fx_loss_account_id,NEW.prepared_by,NEW.created_at)
  IS DISTINCT FROM (OLD.obligation_id,OLD.company_id,OLD.amount_minor,OLD.planned_date,OLD.bank_account,OLD.cash_account_id,OLD.fx_gain_account_id,OLD.fx_loss_account_id,OLD.prepared_by,OLD.created_at)
 THEN RAISE EXCEPTION 'REFUND_INSTRUCTION_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF OLD.export_hash IS NOT NULL AND (NEW.export_hash,NEW.export_command_id,NEW.exported_at) IS DISTINCT FROM (OLD.export_hash,OLD.export_command_id,OLD.exported_at)
 THEN RAISE EXCEPTION 'REFUND_EXPORT_EVIDENCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF OLD.status='confirmed' AND NEW IS DISTINCT FROM OLD
 THEN RAISE EXCEPTION 'REFUND_CONFIRMATION_EVIDENCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER finance_refund_instruction_evidence_guard BEFORE UPDATE OR DELETE ON finance_refund_instructions FOR EACH ROW EXECUTE FUNCTION finance_refund_instruction_evidence_guard();
