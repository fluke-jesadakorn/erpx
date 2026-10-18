CREATE TABLE account_preferences (
  user_id TEXT PRIMARY KEY REFERENCES "user"(id) ON DELETE CASCADE,
  language TEXT NOT NULL CHECK (language IN ('en','th')),
  appearance TEXT NOT NULL CHECK (appearance IN ('system','light','dark')),
  version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
  updated_at BIGINT NOT NULL
);
