-- Reuse folio.ai_chat_sessions for HR agent memory. Nothing new here.
-- Just add an index on (user_id, scope) lookups if missing.
CREATE INDEX IF NOT EXISTS folio_ai_chat_sessions_scope_idx
  ON folio.ai_chat_sessions(user_id, scope)
  WHERE scope LIKE 'hr:%';