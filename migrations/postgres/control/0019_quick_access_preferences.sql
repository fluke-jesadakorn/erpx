CREATE TABLE quick_access_preferences (
  user_id text NOT NULL,
  surface text NOT NULL,
  context_key text NOT NULL,
  pinned_ids_json text,
  version bigint NOT NULL CHECK (version > 0),
  updated_at bigint NOT NULL,
  PRIMARY KEY (user_id, surface, context_key)
);

CREATE TABLE quick_access_preference_receipts (
  id text PRIMARY KEY,
  user_id text NOT NULL,
  surface text NOT NULL,
  context_key text NOT NULL,
  request_hash text NOT NULL,
  result_json text NOT NULL,
  created_at bigint NOT NULL
);

CREATE INDEX quick_access_preference_receipts_scope
  ON quick_access_preference_receipts (user_id, surface, context_key, created_at);
