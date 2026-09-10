CREATE TABLE beta_team_invitations (
 id TEXT PRIMARY KEY, email TEXT NOT NULL, branch_id TEXT NOT NULL REFERENCES organization_nodes(id),
 invited_by TEXT NOT NULL, token_hash TEXT NOT NULL UNIQUE, expires_at INTEGER NOT NULL,
 accepted_user_id TEXT, accepted_at INTEGER, revoked_at INTEGER, created_at INTEGER NOT NULL
) STRICT;
CREATE UNIQUE INDEX beta_pending_email ON beta_team_invitations(email) WHERE accepted_at IS NULL AND revoked_at IS NULL;
