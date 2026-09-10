ALTER TABLE session RENAME TO session_legacy;

CREATE TABLE session (
  id TEXT PRIMARY KEY,
  expiresAt TEXT NOT NULL,
  token TEXT NOT NULL UNIQUE,
  createdAt TEXT NOT NULL,
  updatedAt TEXT NOT NULL,
  ipAddress TEXT,
  userAgent TEXT,
  userId TEXT NOT NULL REFERENCES "user"(id) ON DELETE CASCADE
) STRICT;

INSERT INTO session (
  id, expiresAt, token, createdAt, updatedAt, ipAddress, userAgent, userId
)
SELECT
  id,
  strftime('%Y-%m-%dT%H:%M:%fZ', expiresAt / 1000, 'unixepoch'),
  token,
  strftime('%Y-%m-%dT%H:%M:%fZ', createdAt / 1000, 'unixepoch'),
  strftime('%Y-%m-%dT%H:%M:%fZ', updatedAt / 1000, 'unixepoch'),
  ipAddress,
  userAgent,
  userId
FROM session_legacy;

DROP TABLE session_legacy;
CREATE INDEX session_user_idx ON session(userId);
