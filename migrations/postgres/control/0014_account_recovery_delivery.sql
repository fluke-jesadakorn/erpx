CREATE TABLE account_recovery_delivery_jobs (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
  verification_identifier TEXT NOT NULL UNIQUE,
  url_ciphertext TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'queued' CHECK(state IN ('queued','running','sent','failed','cancelled')),
  attempts BIGINT NOT NULL DEFAULT 0,
  next_attempt_at BIGINT NOT NULL,
  lease_token TEXT,
  lease_until BIGINT,
  provider_id TEXT,
  error TEXT,
  expires_at BIGINT NOT NULL,
  sent_at BIGINT,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL
);
CREATE INDEX account_recovery_delivery_queue
  ON account_recovery_delivery_jobs(state,next_attempt_at,created_at);

CREATE TABLE account_recovery_events (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
  job_id TEXT REFERENCES account_recovery_delivery_jobs(id),
  action TEXT NOT NULL CHECK(action IN ('requested','sent','delivery_failed','cancelled','completed')),
  evidence JSONB NOT NULL DEFAULT '{}',
  created_at BIGINT NOT NULL
);
CREATE INDEX account_recovery_event_user_time
  ON account_recovery_events(user_id,created_at DESC,id DESC);
CREATE TRIGGER account_recovery_events_immutable
  BEFORE UPDATE OR DELETE ON account_recovery_events
  FOR EACH ROW EXECUTE FUNCTION iam_immutable();
