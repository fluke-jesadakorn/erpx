-- Durable, isolated email adapter for Demo and local acceptance runs.
-- It never calls an external provider and is enabled only by an explicit
-- ERPX_EMAIL_ADAPTER=sandbox setting.
CREATE TABLE sandbox_email_deliveries (
  id TEXT PRIMARY KEY,
  delivery_key TEXT NOT NULL UNIQUE,
  recipient TEXT NOT NULL,
  subject TEXT NOT NULL,
  body_text TEXT NOT NULL,
  provider_id TEXT NOT NULL UNIQUE,
  created_at BIGINT NOT NULL,
  metadata_json TEXT NOT NULL DEFAULT '{}' CHECK(erpx_json_valid(metadata_json))
);
CREATE INDEX sandbox_email_deliveries_recipient_time
  ON sandbox_email_deliveries(recipient, created_at DESC, id DESC);
