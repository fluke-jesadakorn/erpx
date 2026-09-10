CREATE TABLE account_invitations (
  id TEXT PRIMARY KEY,token_hash TEXT NOT NULL UNIQUE,holding_id TEXT NOT NULL,company_id TEXT NOT NULL,
  person_id TEXT NOT NULL,email TEXT NOT NULL,name TEXT NOT NULL,role TEXT NOT NULL,
  scope_kind TEXT NOT NULL CHECK(scope_kind IN ('holding','branch')),branch_id TEXT,
  created_by TEXT NOT NULL,assignment_id TEXT NOT NULL,expires_at INTEGER NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','accepted','revoked')),
  accepted_user_id TEXT,created_at INTEGER NOT NULL,accepted_at INTEGER
) STRICT;
CREATE INDEX invitation_person_idx ON account_invitations(holding_id,person_id,status);
