-- Native Account proofs are opaque and single-use. Delivery payloads are encrypted;
-- this outbox queues private work and does not invoke an email provider.
CREATE TABLE native_account_proofs (
  id text PRIMARY KEY,
  user_id text NOT NULL REFERENCES "user"(id),
  kind text NOT NULL CHECK(kind IN ('email_verification','password_reset')),
  token_digest text NOT NULL UNIQUE CHECK(token_digest ~ '^[0-9a-f]{64}$'),
  email text NOT NULL,
  credential_fingerprint text NOT NULL CHECK(credential_fingerprint ~ '^[0-9a-f]{64}$'),
  created_at bigint NOT NULL,
  expires_at bigint NOT NULL CHECK(expires_at>created_at AND expires_at<=created_at+3600000),
  consumed_at bigint
);
CREATE UNIQUE INDEX native_account_current_proof ON native_account_proofs(user_id,kind) WHERE consumed_at IS NULL;
CREATE TABLE native_account_mail_outbox (
  id text PRIMARY KEY,
  proof_id text NOT NULL UNIQUE REFERENCES native_account_proofs(id),
  user_id text NOT NULL REFERENCES "user"(id),
  kind text NOT NULL CHECK(kind IN ('email_verification','password_reset')),
  payload_ciphertext text NOT NULL,
  state text NOT NULL DEFAULT 'queued' CHECK(state IN ('queued','running','sent','failed','cancelled')),
  attempts bigint NOT NULL DEFAULT 0 CHECK(attempts>=0),
  next_attempt_at bigint NOT NULL,
  lease_token text,
  lease_until bigint,
  provider_id text,
  expires_at bigint NOT NULL,
  created_at bigint NOT NULL,
  updated_at bigint NOT NULL
);
CREATE INDEX native_account_mail_queue ON native_account_mail_outbox(state,next_attempt_at,created_at,id);
CREATE TABLE native_account_credential_events (
  id text PRIMARY KEY,
  user_id text NOT NULL REFERENCES "user"(id),
  action text NOT NULL CHECK(action IN ('verification_requested','email_verified','reset_requested','password_reset','password_changed','invitation_credential_created')),
  proof_id text REFERENCES native_account_proofs(id),
  created_at bigint NOT NULL
);
CREATE TRIGGER native_account_credential_events_immutable BEFORE UPDATE OR DELETE ON native_account_credential_events FOR EACH ROW EXECUTE FUNCTION iam_immutable();
