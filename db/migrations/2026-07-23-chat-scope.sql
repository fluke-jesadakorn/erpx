BEGIN;

ALTER TABLE chat.sessions
  ADD COLUMN IF NOT EXISTS scope text NOT NULL DEFAULT 'global';

CREATE INDEX IF NOT EXISTS chat_sessions_scope_idx
  ON chat.sessions(user_id, scope, updated_at DESC);

COMMIT;
