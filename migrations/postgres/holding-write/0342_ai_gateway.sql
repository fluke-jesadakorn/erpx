-- One Holding-local gateway ledger. Never store prompts, completions or credentials.
CREATE TABLE ai_gateway_policies (
  singleton SMALLINT PRIMARY KEY CHECK (singleton=1),
  platform_enabled BOOLEAN NOT NULL DEFAULT TRUE,
  monthly_request_limit INTEGER NOT NULL DEFAULT 1000 CHECK (monthly_request_limit>0),
  monthly_spend_cap_satang BIGINT NOT NULL DEFAULT 10000000 CHECK (monthly_spend_cap_satang>0),
  reservation_satang BIGINT NOT NULL DEFAULT 1000 CHECK (reservation_satang>0),
  updated_by TEXT NOT NULL DEFAULT 'system:migration',
  updated_at BIGINT NOT NULL DEFAULT 0
);
INSERT INTO ai_gateway_policies(singleton) VALUES (1) ON CONFLICT DO NOTHING;

CREATE TABLE ai_gateway_policy_audit (
  id TEXT PRIMARY KEY,
  actor_id TEXT NOT NULL,
  reason TEXT NOT NULL,
  platform_enabled BOOLEAN NOT NULL,
  monthly_request_limit INTEGER NOT NULL,
  monthly_spend_cap_satang BIGINT NOT NULL,
  reservation_satang BIGINT NOT NULL,
  created_at BIGINT NOT NULL
);

CREATE TABLE ai_gateway_requests (
  id TEXT PRIMARY KEY,
  usage_id TEXT NOT NULL,
  holding_id TEXT NOT NULL,
  company_id TEXT NOT NULL,
  branch_id TEXT,
  connection_id TEXT,
  funding_source TEXT NOT NULL CHECK (funding_source IN ('erpx','byok')),
  provider TEXT NOT NULL,
  model TEXT NOT NULL,
  feature TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('reserved','succeeded','failed')),
  input_tokens BIGINT NOT NULL DEFAULT 0 CHECK (input_tokens>=0),
  output_tokens BIGINT NOT NULL DEFAULT 0 CHECK (output_tokens>=0),
  units_json JSONB NOT NULL DEFAULT '{}'::jsonb,
  reservation_satang BIGINT NOT NULL DEFAULT 0 CHECK (reservation_satang>=0),
  created_at BIGINT NOT NULL,
  completed_at BIGINT,
  CHECK ((status='reserved')=(completed_at IS NULL))
);
CREATE INDEX ai_gateway_requests_month_scope ON ai_gateway_requests(created_at,company_id,branch_id);
CREATE INDEX ai_gateway_requests_usage ON ai_gateway_requests(usage_id);
CREATE INDEX ai_gateway_requests_platform_cap ON ai_gateway_requests(created_at,funding_source,status);

CREATE TABLE ai_provider_statements (
  id TEXT PRIMARY KEY,
  holding_id TEXT NOT NULL,
  provider TEXT NOT NULL,
  month TEXT NOT NULL CHECK (month ~ '^\d{4}-(0[1-9]|1[0-2])$'),
  currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
  amount_minor BIGINT NOT NULL CHECK (amount_minor>=0),
  statement_digest TEXT NOT NULL,
  verified_by TEXT NOT NULL,
  verified_at BIGINT NOT NULL,
  reconciled_at BIGINT
);

CREATE TABLE ai_provider_cost_evidence (
  request_id TEXT PRIMARY KEY REFERENCES ai_gateway_requests(id),
  provider_statement_id TEXT NOT NULL REFERENCES ai_provider_statements(id),
  currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
  amount_minor BIGINT NOT NULL CHECK (amount_minor>=0),
  thb_satang BIGINT NOT NULL CHECK (thb_satang>=0),
  fx_evidence_ref TEXT,
  verified_by TEXT NOT NULL,
  verified_at BIGINT NOT NULL,
  reconciled_at BIGINT,
  UNIQUE (provider_statement_id,request_id)
);
