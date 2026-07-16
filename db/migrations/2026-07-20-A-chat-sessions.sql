CREATE TABLE IF NOT EXISTS folio.ai_chat_sessions (
  id bigserial PRIMARY KEY,
  user_id int NOT NULL REFERENCES folio.users(id) ON DELETE CASCADE,
  scope text NOT NULL,
  messages jsonb NOT NULL DEFAULT '[]'::jsonb,
  meta jsonb NOT NULL DEFAULT '{}'::jsonb,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (user_id, scope)
);

CREATE INDEX IF NOT EXISTS folio_ai_chat_sessions_user_idx
  ON folio.ai_chat_sessions(user_id);