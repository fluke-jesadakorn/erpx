-- A split handoff allocates a disjoint interval of the original minor units.
-- Cumulative rounding conserves the original reporting total across all slices.
ALTER TABLE money_transaction_snapshots
 ADD COLUMN carried_allocation_offset_units NUMERIC(38,0),
 ADD CONSTRAINT money_carry_allocation_source CHECK
  (carried_allocation_offset_units IS NULL OR
   (carried_source_type IS NOT NULL AND carried_allocation_offset_units>=0));

CREATE OR REPLACE FUNCTION validate_money_carry() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE source money_transaction_snapshots%ROWTYPE;
BEGIN
 IF NEW.carried_source_type IS NULL THEN RETURN NEW; END IF;
 IF NEW.carried_allocation_offset_units IS NOT NULL THEN
  PERFORM pg_advisory_xact_lock(hashtextextended(concat_ws(':',NEW.company_id,
   NEW.carried_source_type,NEW.carried_source_id,NEW.carried_amount_kind,'carry-allocation'),0));
 END IF;
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
 IF NEW.carried_allocation_offset_units IS NULL THEN
  NEW.usd_units:=round(source.usd_units*NEW.amount_units/source.amount_units);
 ELSE
  IF NEW.carried_allocation_offset_units+NEW.amount_units>source.amount_units THEN
   RAISE EXCEPTION 'MONEY_CARRY_ALLOCATION_EXCEEDS_SOURCE' USING ERRCODE='23514';
  END IF;
  IF EXISTS(SELECT 1 FROM money_transaction_snapshots allocation
   WHERE allocation.company_id=NEW.company_id
    AND allocation.carried_source_type=NEW.carried_source_type
    AND allocation.carried_source_id=NEW.carried_source_id
    AND allocation.carried_amount_kind=NEW.carried_amount_kind
    AND allocation.carried_allocation_offset_units IS NOT NULL
    AND NEW.carried_allocation_offset_units<allocation.carried_allocation_offset_units+allocation.amount_units
    AND allocation.carried_allocation_offset_units<NEW.carried_allocation_offset_units+NEW.amount_units) THEN
   RAISE EXCEPTION 'MONEY_CARRY_ALLOCATION_OVERLAP' USING ERRCODE='23514';
  END IF;
  NEW.usd_units:=round(source.usd_units*(NEW.carried_allocation_offset_units+NEW.amount_units)/source.amount_units)
   -round(source.usd_units*NEW.carried_allocation_offset_units/source.amount_units);
 END IF;
 RETURN NEW;
END $$;
