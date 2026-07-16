BEGIN;

CREATE SCHEMA IF NOT EXISTS chat;

CREATE TABLE chat.sessions (
  id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id      int NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  title        text NOT NULL DEFAULT 'New chat',
  model_name   text NOT NULL DEFAULT 'MiniMax-M3',
  created_at   timestamptz NOT NULL DEFAULT now(),
  updated_at   timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX chat_sessions_user_idx ON chat.sessions(user_id, updated_at DESC);

CREATE TABLE chat.messages (
  id           bigserial PRIMARY KEY,
  session_id   uuid NOT NULL REFERENCES chat.sessions(id) ON DELETE CASCADE,
  role         text NOT NULL CHECK (role IN ('user','assistant','system')),
  content      text NOT NULL,
  blocks       jsonb NOT NULL DEFAULT '{}'::jsonb,
  model_name   text,
  latency_ms   int,
  created_at   timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX chat_messages_session_idx ON chat.messages(session_id, id);

ALTER TABLE chat.sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE chat.messages ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS chat_sessions_self ON chat.sessions;
CREATE POLICY chat_sessions_self ON chat.sessions
  USING (user_id = current_setting('app.user_id', true)::int);
DROP POLICY IF EXISTS chat_messages_self ON chat.messages;
CREATE POLICY chat_messages_self ON chat.messages
  USING (session_id IN (SELECT id FROM chat.sessions WHERE user_id = current_setting('app.user_id', true)::int));

COMMIT;