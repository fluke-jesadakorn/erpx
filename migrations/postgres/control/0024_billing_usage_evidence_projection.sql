-- Billing receives only meter dimensions and cost verification metadata.
-- Provider prompts, responses, files and raw statement documents stay inside
-- the Holding database. NUMERIC preserves exact byte-time without JS overflow.
ALTER TABLE billing_usage_meters
  ADD COLUMN request_status TEXT CHECK (request_status IN ('succeeded','failed')),
  ADD COLUMN gateway_status TEXT CHECK (gateway_status IN ('reserved','succeeded','failed')),
  ADD COLUMN provider TEXT,
  ADD COLUMN model TEXT,
  ADD COLUMN units_json JSONB,
  ADD COLUMN cost_status TEXT NOT NULL DEFAULT 'unavailable'
    CHECK (cost_status IN ('unavailable','unverified','verified','reconciled','not_applicable')),
  ADD COLUMN verified_cost_satang BIGINT CHECK (verified_cost_satang >= 0),
  ADD COLUMN cost_evidence_key TEXT CHECK (cost_evidence_key ~ '^[a-f0-9]{64}$'),
  ADD COLUMN cost_reconciled_at BIGINT,
  ADD COLUMN active_from BIGINT,
  ADD COLUMN active_until BIGINT,
  ADD COLUMN byte_milliseconds NUMERIC(38,0) CHECK (byte_milliseconds >= 0);

CREATE INDEX billing_usage_meters_rating ON billing_usage_meters
  (holding_id, month, meter_kind, funding_source, request_status, cost_status);
