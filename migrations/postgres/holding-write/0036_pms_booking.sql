-- Explicitly opt-in publishing lives in Property config. Historical data is unchanged.
CREATE TABLE pms_booking_receipts (
 id TEXT PRIMARY KEY, property_id TEXT NOT NULL REFERENCES pms_properties(id),
 body_hash TEXT NOT NULL, reservation_id TEXT NOT NULL REFERENCES pms_records(id), created_at BIGINT NOT NULL
);
CREATE TABLE pms_booking_tokens (
 id TEXT PRIMARY KEY, property_id TEXT NOT NULL REFERENCES pms_properties(id),
 reservation_id TEXT NOT NULL REFERENCES pms_records(id), token_hash TEXT NOT NULL UNIQUE,
 expires_at BIGINT NOT NULL, created_at BIGINT NOT NULL
);
CREATE TABLE pms_booking_emails (
 id TEXT PRIMARY KEY, property_id TEXT NOT NULL REFERENCES pms_properties(id),
 reservation_id TEXT NOT NULL REFERENCES pms_records(id), token_id TEXT NOT NULL REFERENCES pms_booking_tokens(id),
 payload_json TEXT NOT NULL CHECK(erpx_json_valid(payload_json)),
 holding_slug TEXT NOT NULL, kind TEXT NOT NULL CHECK(kind IN ('confirmation','cancellation','access')),
 state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','sending','sent','failed')),
 attempts INTEGER NOT NULL DEFAULT 0, next_attempt_at BIGINT NOT NULL,
 lease_until BIGINT, sent_at BIGINT, last_error TEXT, created_at BIGINT NOT NULL
);
CREATE INDEX pms_booking_email_pending ON pms_booking_emails(state,next_attempt_at);
CREATE TABLE pms_booking_limits (
 key TEXT PRIMARY KEY, bucket BIGINT NOT NULL, count INTEGER NOT NULL
);
CREATE INDEX pms_booking_inventory ON pms_records(property_id,kind,(data_json::jsonb->>'state'),(data_json::jsonb->>'arrival'),(data_json::jsonb->>'departure'));
CREATE INDEX pms_booking_reservation ON pms_records(property_id,kind,(data_json::jsonb->>'reservationId'));
