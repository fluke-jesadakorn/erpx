CREATE TABLE pms_booking_payments (
 id TEXT PRIMARY KEY,
 property_id TEXT NOT NULL REFERENCES pms_properties(id),
 reservation_id TEXT NOT NULL UNIQUE REFERENCES pms_records(id),
 company_id TEXT NOT NULL,
 holding_slug TEXT NOT NULL,
 account_id TEXT NOT NULL,
 livemode BOOLEAN NOT NULL,
 amount_minor BIGINT NOT NULL CHECK(amount_minor>0),
 currency TEXT NOT NULL CHECK(currency='THB'),
 checkout_id TEXT UNIQUE,
 checkout_url TEXT,
 status TEXT NOT NULL CHECK(status IN ('payment_pending','confirmed','expired','refund_review','cancelled')),
 expires_at BIGINT NOT NULL,
 paid_at BIGINT,
 refund_state TEXT CHECK(refund_state IN ('requested','approved','processing','succeeded','failed')),
 refund_id TEXT,
 refund_requested_at BIGINT,
 refund_approved_by TEXT,
 last_error TEXT,
 created_at BIGINT NOT NULL,
 updated_at BIGINT NOT NULL
);
CREATE INDEX pms_booking_payments_due ON pms_booking_payments(status,expires_at);
