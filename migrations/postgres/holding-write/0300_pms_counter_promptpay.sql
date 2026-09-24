CREATE TABLE pms_stripe_payments (
 id TEXT PRIMARY KEY,
 request_hash TEXT NOT NULL,
 property_id TEXT NOT NULL REFERENCES pms_properties(id),
 folio_id TEXT NOT NULL REFERENCES pms_records(id),
 company_id TEXT NOT NULL,
 account_id TEXT NOT NULL,
 livemode BOOLEAN NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('receipt','deposit')),
 amount_minor BIGINT NOT NULL CHECK(amount_minor>0),
 payer_email TEXT,
 provider_id TEXT UNIQUE,
 qr_data TEXT,
 state TEXT NOT NULL CHECK(state IN ('requested','pending','succeeded','cancelled','refund_review')),
 entry_id TEXT,
 expires_at BIGINT NOT NULL,
 received_at BIGINT,
 last_error TEXT,
 created_by TEXT NOT NULL,
 created_at BIGINT NOT NULL,
 updated_at BIGINT NOT NULL
);
CREATE UNIQUE INDEX pms_stripe_one_active_folio ON pms_stripe_payments(folio_id) WHERE state IN ('requested','pending');
CREATE INDEX pms_stripe_payments_due ON pms_stripe_payments(state,expires_at);
