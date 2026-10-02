CREATE TABLE account_time_preferences (
  user_id TEXT PRIMARY KEY REFERENCES "user"(id) ON DELETE CASCADE,
  mode TEXT NOT NULL DEFAULT 'system' CHECK (mode IN ('system','manual')),
  time_zone TEXT,
  hour_cycle TEXT NOT NULL DEFAULT 'h23' CHECK (hour_cycle IN ('h12','h23')),
  version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
  updated_at BIGINT NOT NULL,
  CHECK ((mode='system' AND time_zone IS NULL) OR (mode='manual' AND time_zone IS NOT NULL))
);
