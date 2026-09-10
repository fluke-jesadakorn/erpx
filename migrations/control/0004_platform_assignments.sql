CREATE TABLE IF NOT EXISTS platform_assignments (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
  role TEXT NOT NULL CHECK(role LIKE 'platform_%'),
  modules_json TEXT NOT NULL CHECK(json_valid(modules_json)),
  valid_from INTEGER NOT NULL,
  valid_until INTEGER NOT NULL,
  revoked_at INTEGER,
  version INTEGER NOT NULL DEFAULT 1 CHECK(version > 0),
  CHECK(valid_until > valid_from)
) STRICT;

CREATE INDEX IF NOT EXISTS platform_assignment_active_idx
  ON platform_assignments(user_id, valid_from, valid_until);
