-- A Company is discoverable only after its complete minimum organization graph
-- has committed. The receipt is also the recovery point when control-plane
-- binding activation needs to be retried after the holding transaction.
CREATE TABLE company_provisioning (
  company_id TEXT PRIMARY KEY,
  holding_id TEXT NOT NULL UNIQUE,
  owner_user_id TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('provisioning','active','failed')),
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  requested_at BIGINT NOT NULL,
  completed_at BIGINT,
  last_error TEXT,
  updated_at BIGINT NOT NULL
);

CREATE TABLE company_provisioning_events (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  holding_id TEXT NOT NULL,
  action TEXT NOT NULL CHECK(action IN ('started','completed','failed')),
  version BIGINT NOT NULL CHECK(version > 0),
  actor_id TEXT NOT NULL,
  detail_json TEXT NOT NULL DEFAULT '{}' CHECK(erpx_json_valid(detail_json)),
  created_at BIGINT NOT NULL
);
CREATE INDEX company_provisioning_events_company
  ON company_provisioning_events(company_id,created_at,id);
