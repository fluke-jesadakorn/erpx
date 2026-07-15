-- Folio — Hook (webhook receiver) tables
-- Idempotent: safe to re-run.

BEGIN;

CREATE TABLE IF NOT EXISTS hook_providers (
  id            text PRIMARY KEY,
  display_name  text NOT NULL,
  kind          text NOT NULL CHECK (kind IN ('line','generic')),
  secret_env    text NOT NULL,
  enabled       boolean NOT NULL DEFAULT true,
  created_at    timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS hook_events (
  id             bigserial PRIMARY KEY,
  provider_id    text NOT NULL REFERENCES hook_providers(id),
  external_id    text,
  event_type     text NOT NULL,
  received_at    timestamptz NOT NULL DEFAULT now(),
  payload        jsonb NOT NULL,
  headers        jsonb NOT NULL,
  signature_ok   boolean NOT NULL,
  status         text NOT NULL DEFAULT 'received'
                  CHECK (status IN ('received','processed','failed','rejected')),
  processed_at   timestamptz,
  processed_by   text,
  error          text,
  replay_count   int NOT NULL DEFAULT 0,
  UNIQUE (provider_id, external_id)
);

CREATE INDEX IF NOT EXISTS hook_events_received_idx ON hook_events (received_at DESC);
CREATE INDEX IF NOT EXISTS hook_events_status_idx   ON hook_events (status);
CREATE INDEX IF NOT EXISTS hook_events_provider_idx ON hook_events (provider_id);

INSERT INTO hook_providers (id, display_name, kind, secret_env) VALUES
  ('line',    'LINE Official Account', 'line',    'LINE_CHANNEL_SECRET'),
  ('generic', 'Generic Webhook',       'generic', 'HOOK_GENERIC_SECRET')
ON CONFLICT (id) DO NOTHING;

COMMIT;