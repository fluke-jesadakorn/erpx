-- 2026-07-09-D — Opaque session tokens.
-- Replaces the JWT-carried permission list with a server-side sessions table.
-- Cookie = HMAC-signed session id; perms re-queried from perm.effective_user_perms on
-- every request (already the case for node-side hydration).
-- Cookie size drops from ~427 B (CEO/CFO after the previous prune) to ~150 B.

CREATE SCHEMA IF NOT EXISTS auth;

CREATE TABLE auth.sessions (
  id                    text PRIMARY KEY,
  user_id               integer NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  impersonator_user_id  integer REFERENCES users(id) ON DELETE SET NULL,
  role                  text    NOT NULL,
  issued_at             timestamptz NOT NULL DEFAULT now(),
  expires_at            timestamptz NOT NULL,
  revoked_at            timestamptz,
  last_seen_at          timestamptz NOT NULL DEFAULT now(),
  last_seen_ip          inet,
  last_seen_ua          text
);

CREATE INDEX auth_sessions_user_id_idx ON auth.sessions(user_id);
CREATE INDEX auth_sessions_active_idx   ON auth.sessions(expires_at) WHERE revoked_at IS NULL;

-- Nightly cleanup:
--   DELETE FROM auth.sessions WHERE expires_at < now() - interval '7 days';