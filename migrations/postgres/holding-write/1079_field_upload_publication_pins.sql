-- Files-owned, short-lived operational leases for Field upload publication.
-- Source receipts and existing attachment history are neither rewritten nor deleted.
CREATE TABLE field_upload_publication_pins (
  token UUID PRIMARY KEY,
  company_id TEXT NOT NULL,
  upload_intent_id TEXT NOT NULL REFERENCES upload_intents(id),
  object_key TEXT NOT NULL,
  checksum_sha256 TEXT NOT NULL CHECK (checksum_sha256 ~ '^[0-9a-f]{64}$'),
  bytes BIGINT NOT NULL CHECK (bytes BETWEEN 1 AND 10485760),
  created_at BIGINT NOT NULL,
  expires_at BIGINT NOT NULL CHECK (expires_at > created_at),
  state TEXT NOT NULL DEFAULT 'active' CHECK (state IN ('active','committed','released')),
  closed_at BIGINT,
  CHECK ((state = 'active') = (closed_at IS NULL))
);
CREATE INDEX field_upload_publication_pins_live_object ON field_upload_publication_pins(company_id, object_key, expires_at) WHERE state='active';

-- A cleanup claim blocks new pins even after its lease expires. Only a Files
-- worker holding the matching per-object OS lock may reclaim or clear it.
CREATE TABLE field_upload_object_cleanup_claims (
  company_id TEXT NOT NULL,
  object_key TEXT NOT NULL,
  upload_intent_id TEXT NOT NULL REFERENCES upload_intents(id),
  token UUID NOT NULL UNIQUE,
  claimed_at BIGINT NOT NULL,
  claim_until BIGINT NOT NULL CHECK (claim_until > claimed_at),
  object_version TEXT,
  checksum_sha256 TEXT,
  bytes BIGINT,
  CHECK ((object_version IS NULL AND checksum_sha256 IS NULL AND bytes IS NULL) OR
         (object_version IS NOT NULL AND checksum_sha256 ~ '^[0-9a-f]{64}$' AND bytes BETWEEN 1 AND 10485760)),
  PRIMARY KEY(company_id, object_key)
);
