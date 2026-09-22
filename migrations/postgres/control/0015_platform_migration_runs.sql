-- Platform operators need a durable receipt for every schema migration action.
-- The receipt intentionally survives Holding binding removal so operational
-- history remains available during a decommission or recovery investigation.
CREATE TABLE platform_migration_runs (
  id TEXT PRIMARY KEY,
  idempotency_key TEXT NOT NULL UNIQUE,
  request_hash TEXT NOT NULL,
  holding_id TEXT NOT NULL,
  track TEXT NOT NULL CHECK(track IN ('holding-write','holding-read')),
  status TEXT NOT NULL CHECK(status IN ('running','succeeded','failed')),
  requested_by TEXT NOT NULL,
  assignment_id TEXT NOT NULL,
  reason TEXT NOT NULL,
  applied_before BIGINT NOT NULL DEFAULT 0,
  applied_after BIGINT NOT NULL DEFAULT 0,
  result_json TEXT,
  error TEXT,
  requested_at BIGINT NOT NULL,
  started_at BIGINT NOT NULL,
  finished_at BIGINT,
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0)
);

CREATE INDEX platform_migration_runs_holding_idx
  ON platform_migration_runs(holding_id, requested_at DESC);
