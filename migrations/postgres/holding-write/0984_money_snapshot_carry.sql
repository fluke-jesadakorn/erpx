-- Internal handoffs carry an existing positive monetary source and its quote.
-- Reversal provenance remains separate and keeps its cumulative return guard.
ALTER TABLE money_transaction_snapshots
 ADD COLUMN carried_source_type TEXT,
 ADD COLUMN carried_source_id TEXT,
 ADD COLUMN carried_amount_kind TEXT,
 ADD CONSTRAINT money_carry_origin_complete CHECK (
  (carried_source_type IS NULL AND carried_source_id IS NULL AND carried_amount_kind IS NULL) OR
  (carried_source_type IS NOT NULL AND carried_source_id IS NOT NULL AND carried_amount_kind IS NOT NULL)),
 ADD CONSTRAINT money_carry_origin_company_fk FOREIGN KEY
  (company_id,carried_source_type,carried_source_id,carried_amount_kind)
  REFERENCES money_transaction_snapshots(company_id,source_type,source_id,amount_kind);

CREATE FUNCTION validate_money_carry() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE source money_transaction_snapshots%ROWTYPE;
BEGIN
 IF NEW.carried_source_type IS NULL THEN RETURN NEW; END IF;
 SELECT * INTO source FROM money_transaction_snapshots WHERE company_id=NEW.company_id
  AND source_type=NEW.carried_source_type AND source_id=NEW.carried_source_id
  AND amount_kind=NEW.carried_amount_kind;
 IF NOT FOUND OR source.amount_units<=0 OR source.original_source_type IS NOT NULL OR
  NEW.original_source_type IS NOT NULL OR NEW.amount_units<0 OR NEW.amount_units>source.amount_units THEN
  RAISE EXCEPTION 'MONEY_CARRY_SOURCE_INVALID' USING ERRCODE='23514';
 END IF;
 IF NEW.currency<>source.currency OR NEW.currency_scale<>source.currency_scale OR
  NEW.fx_rate_id IS DISTINCT FROM source.fx_rate_id OR NEW.used_latest_fallback<>source.used_latest_fallback THEN
  RAISE EXCEPTION 'MONEY_CARRY_EVIDENCE_MISMATCH' USING ERRCODE='23514';
 END IF;
 NEW.usd_units:=round(source.usd_units*NEW.amount_units/source.amount_units);
 RETURN NEW;
END $$;
CREATE TRIGGER validate_money_carry_before_insert BEFORE INSERT ON money_transaction_snapshots
 FOR EACH ROW EXECUTE FUNCTION validate_money_carry();
