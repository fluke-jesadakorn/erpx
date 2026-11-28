-- Work projection owner writes execute their idempotency receipt, audit and
-- outbox ledger in the same read-model transaction as `work_items`. The write
-- track already owns the platform columns; this track keeps the same shape so
-- the native Work owner can commit without a second database.
CREATE TABLE command_receipts (
  command_id TEXT PRIMARY KEY,
  operation TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  request_hash TEXT NOT NULL,
  record_id TEXT NOT NULL,
  result_json TEXT,
  created_at BIGINT NOT NULL,
  authority_json TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE audit_events (
  id TEXT PRIMARY KEY,
  command_id TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  assignment_id TEXT NOT NULL,
  action TEXT NOT NULL,
  record_type TEXT NOT NULL,
  record_id TEXT NOT NULL,
  record_version BIGINT NOT NULL,
  purpose TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  authority_json TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE outbox_events (
  id TEXT PRIMARY KEY,
  command_id TEXT NOT NULL,
  event_type TEXT NOT NULL,
  aggregate_type TEXT NOT NULL,
  aggregate_id TEXT NOT NULL,
  aggregate_version BIGINT NOT NULL,
  payload_json TEXT NOT NULL CHECK(erpx_json_valid(payload_json)),
  created_at BIGINT NOT NULL,
  published_at BIGINT,
  claim_until BIGINT,
  attempts BIGINT NOT NULL DEFAULT 0
);
