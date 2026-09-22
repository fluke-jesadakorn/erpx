CREATE TABLE account_invitation_delivery_jobs (
  id TEXT PRIMARY KEY,
  invitation_id TEXT NOT NULL UNIQUE REFERENCES account_invitations(id),
  holding_id TEXT NOT NULL,
  company_id TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'awaiting_approval' CHECK(state IN ('awaiting_approval','queued','running','sent','failed','cancelled')),
  attempts BIGINT NOT NULL DEFAULT 0,
  next_attempt_at BIGINT NOT NULL,
  lease_token TEXT,
  lease_until BIGINT,
  provider_id TEXT,
  error TEXT,
  sent_at BIGINT,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL
);
CREATE INDEX account_invitation_delivery_queue
  ON account_invitation_delivery_jobs(state,next_attempt_at,created_at);
