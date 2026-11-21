-- Native browser IDs remain stable. Only a presented opaque cookie's purpose-
-- separated digest resolves a key; the stored ID is never a replacement secret.
CREATE TABLE native_account_device_group_keys (
  cookie_digest text PRIMARY KEY CHECK(cookie_digest ~ '^[0-9a-f]{64}$'),
  browser_id text NOT NULL REFERENCES account_device_browsers(id) ON DELETE CASCADE
    CHECK(browser_id ~ '^[0-9a-f]{64}$'),
  created_at bigint NOT NULL,
  revoked_at bigint
);
CREATE UNIQUE INDEX native_account_device_group_current_key
  ON native_account_device_group_keys(browser_id) WHERE revoked_at IS NULL;
-- Explicit fresh-native compatibility: existing native groups used the cookie
-- digest as their ID. UUID/signed Better Auth groups are not adopted or granted.
INSERT INTO native_account_device_group_keys(cookie_digest,browser_id,created_at)
  SELECT id,id,updated_at FROM account_device_browsers WHERE id ~ '^[0-9a-f]{64}$';
