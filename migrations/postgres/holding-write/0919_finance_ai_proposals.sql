-- Proposals are advisory. Financial owner commands retain all posting/payment authority.
CREATE TABLE finance_ai_policies (
  company_id TEXT PRIMARY KEY REFERENCES organization_nodes(id),
  enabled BOOLEAN NOT NULL DEFAULT FALSE,
  version BIGINT NOT NULL CHECK(version>0),
  kinds_json TEXT NOT NULL CHECK(erpx_json_valid(kinds_json)),
  timezone TEXT NOT NULL,
  daily_limit BIGINT NOT NULL CHECK(daily_limit BETWEEN 1 AND 50),
  monthly_limit BIGINT NOT NULL CHECK(monthly_limit BETWEEN 1 AND 1000),
  owner_actor_json TEXT NOT NULL CHECK(erpx_json_valid(owner_actor_json)),
  reason TEXT NOT NULL,
  changed_by TEXT NOT NULL,
  changed_at BIGINT NOT NULL,
  next_scan_at BIGINT NOT NULL DEFAULT 0,
  last_error TEXT
);
CREATE TABLE finance_ai_proposals (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL REFERENCES organization_nodes(id),
  kind TEXT NOT NULL CHECK(kind IN ('today_brief','invoice_review','collection_draft','payment_preparation','bank_match','tax_review','close_review','report_explanation')),
  source_id TEXT NOT NULL,
  source_version BIGINT NOT NULL,
  source_hash TEXT NOT NULL,
  source_json TEXT NOT NULL CHECK(erpx_json_valid(source_json)),
  source_query_json TEXT NOT NULL CHECK(erpx_json_valid(source_query_json)),
  company_revision BIGINT NOT NULL,
  policy_version BIGINT NOT NULL,
  provider_json TEXT NOT NULL CHECK(erpx_json_valid(provider_json)),
  actor_json TEXT NOT NULL CHECK(erpx_json_valid(actor_json)),
  state TEXT NOT NULL CHECK(state IN ('queued','running','ready','failed','stale','dismissed','applied')),
  version BIGINT NOT NULL DEFAULT 1,
  result_json TEXT CHECK(result_json IS NULL OR erpx_json_valid(result_json)),
  error TEXT,
  attempts BIGINT NOT NULL DEFAULT 0,
  lease_token TEXT,
  claim_until BIGINT,
  draft_command_id TEXT,
  usage_id TEXT,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  UNIQUE(company_id,kind,source_id,source_hash,provider_json,policy_version)
);
CREATE TABLE finance_ai_usage (
  id TEXT PRIMARY KEY,
  proposal_id TEXT NOT NULL REFERENCES finance_ai_proposals(id),
  company_id TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  month TEXT NOT NULL,
  business_date TEXT NOT NULL,
  state TEXT NOT NULL CHECK(state IN ('reserved','succeeded','failed')),
  input_tokens BIGINT NOT NULL DEFAULT 0,
  output_tokens BIGINT NOT NULL DEFAULT 0,
  lease_until BIGINT NOT NULL,
  created_at BIGINT NOT NULL
);
CREATE TABLE finance_ai_events (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  proposal_id TEXT REFERENCES finance_ai_proposals(id),
  action TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  command_id TEXT,
  evidence_json TEXT NOT NULL CHECK(erpx_json_valid(evidence_json)),
  created_at BIGINT NOT NULL
);
CREATE INDEX finance_ai_queue ON finance_ai_proposals(state,claim_until,created_at) WHERE state IN ('queued','running');
CREATE INDEX finance_ai_company_view ON finance_ai_proposals(company_id,updated_at DESC);
CREATE INDEX finance_ai_quota ON finance_ai_usage(company_id,month,business_date);
CREATE INDEX finance_ai_policy_scan ON finance_ai_policies(next_scan_at) WHERE enabled;

-- Reuse private-file OCR and current-stage ownership for automatic receipt preparation.
ALTER TABLE expense_ai_proposals ADD COLUMN finance_policy_version BIGINT;
CREATE INDEX finance_ai_receipt_policy ON expense_ai_proposals(company_id,created_at) WHERE finance_policy_version IS NOT NULL;
CREATE INDEX finance_ai_source_revision ON command_receipts ((authority_json::jsonb->>'companyId'),operation);
