CREATE TABLE inventory_count_decisions (
  id TEXT PRIMARY KEY,
  count_id TEXT NOT NULL REFERENCES inventory_counts(id),
  command_id TEXT NOT NULL UNIQUE REFERENCES command_receipts(command_id),
  action TEXT NOT NULL CHECK(action IN ('post','reject','cancel')),
  actor_id TEXT NOT NULL,
  reviewed_version INTEGER NOT NULL,
  reason TEXT NOT NULL,
  created_at INTEGER NOT NULL
) STRICT;
CREATE INDEX inventory_count_decisions_count_idx ON inventory_count_decisions(count_id,created_at);
