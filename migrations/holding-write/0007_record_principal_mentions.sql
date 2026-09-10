CREATE TABLE IF NOT EXISTS record_principal_mentions (
  host_module TEXT NOT NULL,
  host_id TEXT NOT NULL,
  target_kind TEXT NOT NULL CHECK(target_kind IN ('person','group','agent_pool')),
  target_id TEXT NOT NULL,
  target_label TEXT NOT NULL,
  created_by TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  PRIMARY KEY(host_module, host_id, target_kind, target_id),
  FOREIGN KEY(host_module, host_id) REFERENCES workspace_records(module_id, id)
) STRICT;
CREATE INDEX IF NOT EXISTS record_principal_mentions_target_idx
  ON record_principal_mentions(target_kind, target_id, created_at DESC);
