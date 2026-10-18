-- Metadata is written only at session creation. Existing sessions stay legacy.
-- No FK on metadata: Better Auth's after-create hook may run before its own
-- transaction commits. Every read joins the live session; an orphan authorizes nothing.
CREATE TABLE sso_session_bindings (
  session_id text PRIMARY KEY,
  audience text NOT NULL,
  environment text NOT NULL CHECK(environment='development'),
  parent_session_id text,
  authenticated_at bigint NOT NULL,
  absolute_expires_at bigint NOT NULL CHECK(absolute_expires_at>authenticated_at),
  CHECK(absolute_expires_at<=authenticated_at+43200000)
);
CREATE INDEX sso_session_parent ON sso_session_bindings(parent_session_id);
CREATE TABLE sso_browser_transactions (
  id text PRIMARY KEY,
  browser_hash text NOT NULL,
  state_hash text NOT NULL,
  challenge text NOT NULL,
  audience text NOT NULL,
  environment text NOT NULL CHECK(environment='development'),
  callback text NOT NULL,
  return_path text NOT NULL,
  created_at bigint NOT NULL,
  expires_at bigint NOT NULL,
  consumed_at bigint,
  CHECK(expires_at>created_at AND expires_at<=created_at+300000)
);
CREATE TABLE sso_authorization_grants (
  code_hash text PRIMARY KEY,
  transaction_id text NOT NULL REFERENCES sso_browser_transactions(id),
  parent_session_id text NOT NULL,
  audience text NOT NULL,
  environment text NOT NULL CHECK(environment='development'),
  callback text NOT NULL,
  state_hash text NOT NULL,
  challenge text NOT NULL,
  created_at bigint NOT NULL,
  expires_at bigint NOT NULL,
  consumed_at bigint,
  CHECK(expires_at>created_at AND expires_at<=created_at+60000)
);
CREATE INDEX sso_grant_expiry ON sso_authorization_grants(expires_at);
CREATE INDEX sso_transaction_expiry ON sso_browser_transactions(expires_at);
