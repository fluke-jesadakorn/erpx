ALTER TABLE folio.waybills
  ADD COLUMN IF NOT EXISTS flagged_reason jsonb;

CREATE INDEX IF NOT EXISTS folio_waybills_flagged_reason_idx
  ON folio.waybills USING gin (flagged_reason)
  WHERE flagged_reason IS NOT NULL;