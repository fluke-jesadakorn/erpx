CREATE TABLE IF NOT EXISTS platform_health (
  component TEXT PRIMARY KEY,
  status TEXT NOT NULL,
  checked_at INTEGER NOT NULL,
  detail TEXT
) STRICT;
