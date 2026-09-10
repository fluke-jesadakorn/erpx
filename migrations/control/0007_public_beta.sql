CREATE TABLE beta_slots (
  holding_id TEXT PRIMARY KEY, owner_user_id TEXT UNIQUE REFERENCES user(id),
  slug TEXT UNIQUE, company_name TEXT, branch_name TEXT,
  state TEXT NOT NULL DEFAULT 'available' CHECK(state IN ('available','provisioning','active')),
  created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
) STRICT;
CREATE TABLE beta_waitlist (user_id TEXT PRIMARY KEY REFERENCES user(id), created_at INTEGER NOT NULL) STRICT;
CREATE TABLE beta_invitations (
  id TEXT PRIMARY KEY, holding_id TEXT NOT NULL, email TEXT NOT NULL, branch_id TEXT NOT NULL,
  invited_by TEXT NOT NULL, token_hash TEXT NOT NULL UNIQUE, expires_at INTEGER NOT NULL,
  accepted_user_id TEXT, accepted_at INTEGER, revoked_at INTEGER, created_at INTEGER NOT NULL
) STRICT;
CREATE INDEX beta_invitation_email ON beta_invitations(email,holding_id);
CREATE TABLE rateLimit (id TEXT PRIMARY KEY, key TEXT NOT NULL UNIQUE, count INTEGER NOT NULL, lastRequest INTEGER NOT NULL) STRICT;
CREATE TABLE beta_billing (
  holding_id TEXT PRIMARY KEY, customer_id TEXT UNIQUE, subscription_id TEXT UNIQUE,
  status TEXT NOT NULL DEFAULT 'free_beta', current_period_end INTEGER,
  last_event_at INTEGER NOT NULL DEFAULT 0, updated_at INTEGER NOT NULL
) STRICT;
CREATE TABLE stripe_inbox (
  id TEXT PRIMARY KEY, event_type TEXT NOT NULL, created_at INTEGER NOT NULL,
  state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','applied','ignored')),
  payload_json TEXT NOT NULL CHECK(json_valid(payload_json)), applied_at INTEGER
) STRICT;
