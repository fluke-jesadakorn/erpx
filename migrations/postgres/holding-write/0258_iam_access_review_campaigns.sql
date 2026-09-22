CREATE TABLE iam_access_review_campaigns (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  name TEXT NOT NULL,
  due_at BIGINT NOT NULL,
  status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','completed','cancelled')),
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  created_by TEXT NOT NULL,
  reason TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  completed_at BIGINT
);
CREATE INDEX iam_access_review_campaign_queue
  ON iam_access_review_campaigns(company_id,status,due_at,created_at DESC);

CREATE TABLE iam_access_review_items (
  id TEXT PRIMARY KEY,
  campaign_id TEXT NOT NULL REFERENCES iam_access_review_campaigns(id),
  company_id TEXT NOT NULL,
  assignment_id TEXT NOT NULL REFERENCES assignments(id),
  assignment_version BIGINT NOT NULL,
  user_id TEXT NOT NULL,
  role TEXT NOT NULL,
  scope_kind TEXT NOT NULL,
  scope_node_id TEXT,
  source TEXT NOT NULL,
  valid_until BIGINT NOT NULL,
  decision TEXT NOT NULL DEFAULT 'pending' CHECK(decision IN ('pending','keep','revoke')),
  decided_by TEXT,
  decision_reason TEXT,
  decided_at BIGINT,
  UNIQUE(campaign_id,assignment_id)
);
CREATE INDEX iam_access_review_item_queue
  ON iam_access_review_items(company_id,campaign_id,decision,user_id);

CREATE TABLE iam_access_review_events (
  id TEXT PRIMARY KEY,
  campaign_id TEXT NOT NULL REFERENCES iam_access_review_campaigns(id),
  item_id TEXT REFERENCES iam_access_review_items(id),
  company_id TEXT NOT NULL,
  action TEXT NOT NULL CHECK(action IN ('started','kept','revoked','completed','cancelled')),
  actor_id TEXT NOT NULL,
  snapshot_json TEXT NOT NULL CHECK(erpx_json_valid(snapshot_json)),
  created_at BIGINT NOT NULL
);
CREATE INDEX iam_access_review_event_history
  ON iam_access_review_events(campaign_id,created_at,id);

CREATE FUNCTION iam_access_review_event_immutable_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'IMMUTABLE_IAM_ACCESS_REVIEW_EVENT' USING ERRCODE='23514';
END $$;
CREATE TRIGGER iam_access_review_event_immutable
  BEFORE UPDATE OR DELETE ON iam_access_review_events
  FOR EACH ROW EXECUTE FUNCTION iam_access_review_event_immutable_fn();
