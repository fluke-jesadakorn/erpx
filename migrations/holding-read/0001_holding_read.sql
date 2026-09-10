CREATE TABLE IF NOT EXISTS work_items (
  id TEXT PRIMARY KEY,
  source_type TEXT NOT NULL,
  source_id TEXT NOT NULL,
  source_version INTEGER NOT NULL,
  title TEXT NOT NULL,
  subtitle TEXT NOT NULL,
  queue TEXT NOT NULL CHECK(queue IN ('todo','waiting','completed')),
  assigned_role TEXT,
  assigned_user_id TEXT,
  company_id TEXT NOT NULL,
  branch_id TEXT,
  due_at INTEGER,
  updated_at INTEGER NOT NULL,
  UNIQUE(source_type, source_id)
) STRICT;
CREATE INDEX IF NOT EXISTS work_queue_idx ON work_items(queue, assigned_user_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS projection_receipts (
  event_id TEXT PRIMARY KEY,
  aggregate_id TEXT NOT NULL,
  aggregate_version INTEGER NOT NULL,
  applied_at INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS projection_checkpoints (
  projector TEXT PRIMARY KEY,
  last_event_id TEXT,
  last_event_at INTEGER,
  updated_at INTEGER NOT NULL
) STRICT;
