CREATE TABLE IF NOT EXISTS social_mentions (
  owner_type TEXT NOT NULL CHECK(owner_type IN ('post','comment')),
  owner_id TEXT NOT NULL,
  target_kind TEXT NOT NULL CHECK(target_kind IN ('person','record')),
  target_id TEXT NOT NULL,
  target_label TEXT NOT NULL,
  created_by TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  PRIMARY KEY(owner_type, owner_id, target_kind, target_id)
) STRICT;
CREATE INDEX IF NOT EXISTS social_mentions_target_idx
  ON social_mentions(target_kind, target_id, created_at DESC);

ALTER TABLE workspace_records ADD COLUMN archived_at INTEGER;
