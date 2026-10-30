-- Browser-local selection. Saved Account sessions remain live when switching;
-- their child sessions are removed under the existing session-first fence.
CREATE TABLE account_device_browsers (
  id text PRIMARY KEY,
  active_session_id text,
  revision bigint NOT NULL DEFAULT 0 CHECK (revision >= 0),
  pending_nonce text,
  pending_until bigint,
  updated_at bigint NOT NULL
);
CREATE TABLE account_device_accounts (
  session_id text PRIMARY KEY,
  browser_id text NOT NULL REFERENCES account_device_browsers(id) ON DELETE CASCADE,
  handle text NOT NULL UNIQUE
);
CREATE INDEX account_device_accounts_browser ON account_device_accounts(browser_id);
-- No session FK: auth after-create and original family deletion retain their
-- transaction ordering. Every use joins a live, audience-bound Account session.
