CREATE TABLE company_business_acceptance (
  company_id TEXT PRIMARY KEY REFERENCES organization_nodes(id),
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','submitted','rejected','accepted','revoked')),
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  requested_by TEXT NOT NULL,
  reviewed_by TEXT,
  requested_at BIGINT NOT NULL,
  reviewed_at BIGINT,
  reason TEXT NOT NULL CHECK(length(btrim(reason)) BETWEEN 3 AND 2000),
  review_reason TEXT,
  updated_at BIGINT NOT NULL
);

CREATE TABLE company_business_acceptance_events (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL REFERENCES organization_nodes(id),
  action TEXT NOT NULL CHECK(action IN ('submitted','rejected','accepted','revoked')),
  actor_id TEXT NOT NULL,
  version BIGINT NOT NULL CHECK(version > 0),
  reason TEXT NOT NULL CHECK(length(btrim(reason)) BETWEEN 3 AND 2000),
  created_at BIGINT NOT NULL
);

CREATE INDEX company_business_acceptance_events_company
  ON company_business_acceptance_events(company_id, created_at, id);
