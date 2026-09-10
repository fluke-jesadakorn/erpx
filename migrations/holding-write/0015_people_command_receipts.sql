CREATE TABLE IF NOT EXISTS hr_command_receipts (
  command_id TEXT PRIMARY KEY,
  operation TEXT NOT NULL,
  request_hash TEXT NOT NULL,
  result_json TEXT,
  created_at INTEGER NOT NULL
) STRICT;
