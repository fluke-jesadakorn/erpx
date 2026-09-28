-- Billing meters are copied from isolated Holding databases without business
-- content. A source digest makes repeated projection safe; a snapshot marker
-- removes meters that were corrected or destroyed after an earlier refresh.
CREATE TABLE billing_usage_snapshots (
  holding_id TEXT NOT NULL REFERENCES holding_bindings(holding_id),
  month TEXT NOT NULL CHECK (month ~ '^[0-9]{4}-(0[1-9]|1[0-2])$'),
  snapshot_id TEXT NOT NULL,
  refreshed_at BIGINT NOT NULL,
  source_count INTEGER NOT NULL DEFAULT 0 CHECK (source_count >= 0),
  ai_coverage TEXT NOT NULL DEFAULT 'partial' CHECK (ai_coverage IN ('measured','partial')),
  storage_coverage TEXT NOT NULL DEFAULT 'partial' CHECK (storage_coverage IN ('measured','partial')),
  unreconciled_originals INTEGER NOT NULL DEFAULT 0 CHECK (unreconciled_originals >= 0),
  PRIMARY KEY (holding_id, month)
);

CREATE TABLE billing_usage_meters (
  holding_id TEXT NOT NULL,
  month TEXT NOT NULL,
  source_key TEXT NOT NULL CHECK (source_key ~ '^[a-f0-9]{64}$'),
  company_id TEXT NOT NULL,
  company_name TEXT NOT NULL,
  branch_id TEXT,
  branch_name TEXT,
  meter_kind TEXT NOT NULL CHECK (meter_kind IN ('ai','storage')),
  funding_source TEXT NOT NULL CHECK (funding_source IN ('erpx','byok','simulation','unknown')),
  pricing_status TEXT NOT NULL CHECK (pricing_status IN ('byok','simulation','unpriced')),
  quantity BIGINT NOT NULL CHECK (quantity >= 0),
  input_tokens BIGINT NOT NULL DEFAULT 0 CHECK (input_tokens >= 0),
  output_tokens BIGINT NOT NULL DEFAULT 0 CHECK (output_tokens >= 0),
  average_daily_bytes BIGINT NOT NULL DEFAULT 0 CHECK (average_daily_bytes >= 0),
  active_at_period_end BOOLEAN NOT NULL DEFAULT FALSE,
  snapshot_id TEXT NOT NULL,
  PRIMARY KEY (holding_id, month, source_key),
  FOREIGN KEY (holding_id, month) REFERENCES billing_usage_snapshots(holding_id, month) ON DELETE CASCADE
);

CREATE INDEX billing_usage_meters_month_holding ON billing_usage_meters(month, holding_id, company_id, branch_id);
