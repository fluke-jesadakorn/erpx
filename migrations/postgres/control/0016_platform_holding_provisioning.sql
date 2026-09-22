-- Platform provisioning receipts describe the control-plane binding lifecycle.
-- A Holding database is pre-provisioned by the operator; this record is the
-- audited, transactional activation of its public ERPX binding.
CREATE TABLE platform_holding_provisioning (
  id TEXT PRIMARY KEY,
  holding_id TEXT NOT NULL UNIQUE,
  slug TEXT NOT NULL UNIQUE,
  display_name TEXT NOT NULL,
  write_binding TEXT NOT NULL UNIQUE,
  read_binding TEXT NOT NULL UNIQUE,
  files_binding TEXT NOT NULL UNIQUE,
  status TEXT NOT NULL CHECK(status IN ('provisioning','active','failed','suspended')),
  requested_by TEXT NOT NULL,
  assignment_id TEXT NOT NULL,
  reason TEXT NOT NULL,
  request_hash TEXT NOT NULL,
  idempotency_key TEXT NOT NULL UNIQUE,
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  error TEXT,
  requested_at BIGINT NOT NULL,
  activated_at BIGINT,
  updated_at BIGINT NOT NULL
);

CREATE INDEX platform_holding_provisioning_status
  ON platform_holding_provisioning(status, updated_at DESC);

CREATE TABLE platform_holding_provisioning_events (
  id TEXT PRIMARY KEY,
  provisioning_id TEXT NOT NULL REFERENCES platform_holding_provisioning(id),
  holding_id TEXT NOT NULL,
  action TEXT NOT NULL CHECK(action IN ('started','validated','activated','retry_started','failed','suspended')),
  actor_id TEXT NOT NULL,
  assignment_id TEXT NOT NULL,
  reason TEXT NOT NULL,
  detail_json TEXT NOT NULL DEFAULT '{}' CHECK(erpx_json_valid(detail_json)),
  created_at BIGINT NOT NULL
);

CREATE INDEX platform_holding_provisioning_events_lookup
  ON platform_holding_provisioning_events(holding_id, created_at DESC, id DESC);

CREATE TRIGGER platform_holding_provisioning_events_immutable
  BEFORE UPDATE OR DELETE ON platform_holding_provisioning_events
  FOR EACH ROW EXECUTE FUNCTION iam_immutable();
