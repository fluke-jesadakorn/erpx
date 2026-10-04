-- Add provenance without changing the meaning or bytes of any historical snapshot.
ALTER TABLE money_transaction_snapshots
  ADD COLUMN original_source_type TEXT,
  ADD COLUMN original_source_id TEXT,
  ADD COLUMN original_amount_kind TEXT,
  ADD CONSTRAINT money_reversal_origin_complete CHECK (
    (original_source_type IS NULL AND original_source_id IS NULL AND original_amount_kind IS NULL) OR
    (original_source_type IS NOT NULL AND original_source_id IS NOT NULL AND original_amount_kind IS NOT NULL)),
  ADD CONSTRAINT money_reversal_origin_company_fk FOREIGN KEY
    (company_id,original_source_type,original_source_id,original_amount_kind)
    REFERENCES money_transaction_snapshots(company_id,source_type,source_id,amount_kind);

CREATE INDEX money_reversal_origin ON money_transaction_snapshots
  (company_id,original_source_type,original_source_id,original_amount_kind)
  WHERE original_source_type IS NOT NULL;

CREATE FUNCTION allocate_money_reversal() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE source money_transaction_snapshots%ROWTYPE;
DECLARE previous_units NUMERIC(38,0);
DECLARE previous_usd NUMERIC(38,0);
DECLARE next_units NUMERIC(38,0);
BEGIN
  IF NEW.original_source_type IS NULL THEN RETURN NEW; END IF;
  -- Serialize the immutable origin without granting UPDATE to the application.
  -- Advisory collisions only serialize unrelated origins; they cannot loosen a guard.
  PERFORM pg_advisory_xact_lock(hashtextextended(jsonb_build_array(
    NEW.company_id,NEW.original_source_type,NEW.original_source_id,NEW.original_amount_kind)::text,0));
  SELECT * INTO source FROM money_transaction_snapshots WHERE
    company_id=NEW.company_id AND source_type=NEW.original_source_type AND
    source_id=NEW.original_source_id AND amount_kind=NEW.original_amount_kind;
  IF NOT FOUND OR source.original_source_type IS NOT NULL OR source.amount_units<=0 OR source.usd_units<0 THEN
    RAISE EXCEPTION 'MONEY_REVERSAL_SOURCE_INVALID' USING ERRCODE='23514';
  END IF;
  IF NEW.amount_units>=0 OR NEW.currency<>source.currency OR NEW.currency_scale<>source.currency_scale OR
    NEW.fx_rate_id IS DISTINCT FROM source.fx_rate_id OR NEW.used_latest_fallback<>source.used_latest_fallback THEN
    RAISE EXCEPTION 'MONEY_REVERSAL_EVIDENCE_MISMATCH' USING ERRCODE='23514';
  END IF;
  SELECT COALESCE(-SUM(amount_units),0),COALESCE(-SUM(usd_units),0) INTO previous_units,previous_usd
    FROM money_transaction_snapshots WHERE company_id=NEW.company_id AND
    original_source_type=NEW.original_source_type AND original_source_id=NEW.original_source_id AND
    original_amount_kind=NEW.original_amount_kind;
  next_units:=previous_units-NEW.amount_units;
  IF next_units>source.amount_units THEN
    RAISE EXCEPTION 'MONEY_REVERSAL_EXCEEDS_SOURCE' USING ERRCODE='23514';
  END IF;
  NEW.usd_units:=-(round(source.usd_units*next_units/source.amount_units)-previous_usd);
  RETURN NEW;
END $$;

CREATE TRIGGER allocate_money_reversal_before_insert BEFORE INSERT ON money_transaction_snapshots
  FOR EACH ROW EXECUTE FUNCTION allocate_money_reversal();
