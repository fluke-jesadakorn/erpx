CREATE TABLE member_profiles (
  user_id TEXT PRIMARY KEY,
  bio TEXT NOT NULL DEFAULT '' CHECK (length(bio) <= 500),
  links_json JSONB NOT NULL DEFAULT '[]'::jsonb,
  avatar_key TEXT,
  cover_key TEXT,
  avatar_media_type TEXT,
  cover_media_type TEXT,
  version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
  updated_at BIGINT NOT NULL
);
