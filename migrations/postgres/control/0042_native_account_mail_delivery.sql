-- Each provider invocation has durable intent. SMTP ambiguity must never be
-- mistaken for an unstarted job and automatically resent after a worker crash.
ALTER TABLE native_account_mail_outbox
  ADD COLUMN provider_kind text CHECK(provider_kind IN ('resend','smtp')),
  ADD COLUMN provider_profile_id text,
  ADD COLUMN message_fingerprint text CHECK(message_fingerprint ~ '^[0-9a-f]{64}$'),
  ADD COLUMN dispatch_started_at bigint,
  ADD COLUMN idempotency_started_at bigint,
  ADD COLUMN dispatch_lease_digest text CHECK(dispatch_lease_digest ~ '^[0-9a-f]{64}$'),
  ADD COLUMN failure_code text,
  ADD COLUMN sent_at bigint,
  ADD CONSTRAINT native_account_mail_provider_intent CHECK (
    dispatch_started_at IS NULL OR
    provider_kind IS NOT NULL AND provider_profile_id IS NOT NULL AND message_fingerprint IS NOT NULL AND idempotency_started_at IS NOT NULL AND dispatch_lease_digest IS NOT NULL
  ),
  ADD CONSTRAINT native_account_mail_accepted_receipt CHECK (
    state<>'sent' OR
    provider_id IS NOT NULL AND length(provider_id)>0 AND sent_at IS NOT NULL AND dispatch_started_at IS NOT NULL
  );
CREATE TABLE native_account_mail_events (
  id text PRIMARY KEY,
  outbox_id text NOT NULL REFERENCES native_account_mail_outbox(id),
  attempt bigint NOT NULL CHECK(attempt>=1 AND attempt<=6),
  action text NOT NULL CHECK(action IN ('claimed','dispatch_started','accepted','retry_scheduled','cancelled','failed','uncertain')),
  failure_code text,
  created_at bigint NOT NULL,
  UNIQUE(outbox_id,attempt,action)
);
CREATE TRIGGER native_account_mail_events_immutable BEFORE UPDATE OR DELETE ON native_account_mail_events
  FOR EACH ROW EXECUTE FUNCTION iam_immutable();
