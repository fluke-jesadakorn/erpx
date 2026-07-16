CREATE TABLE IF NOT EXISTS folio.notification_digests (
  id bigserial PRIMARY KEY,
  user_id int NOT NULL REFERENCES folio.users(id),
  period_start timestamptz NOT NULL,
  period_end timestamptz NOT NULL,
  severity text NOT NULL,
  bullets jsonb NOT NULL,
  source_count int NOT NULL,
  generated_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (user_id, period_start, period_end)
);

CREATE INDEX IF NOT EXISTS folio_notification_digests_user_idx
  ON folio.notification_digests(user_id, generated_at DESC);