CREATE TABLE account_invitation_claims (
  invitation_id TEXT PRIMARY KEY REFERENCES account_invitations(id),
  eligible INTEGER NOT NULL CHECK(eligible=1),
  claimed_at INTEGER NOT NULL
) STRICT;
