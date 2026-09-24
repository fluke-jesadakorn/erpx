-- Preserve existing POS payment and accounting references while making the
-- company connection usable by every sales channel.
ALTER TABLE pos_connections ADD COLUMN payout_interval TEXT;
ALTER TABLE pos_connections ADD COLUMN thb_bank_ready BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE pos_connections ADD COLUMN last_checked_at BIGINT;
ALTER TABLE pos_payments ADD COLUMN qr_data TEXT;
ALTER TABLE pos_payments ADD COLUMN expires_at BIGINT;
ALTER TABLE pos_payments ADD COLUMN payer_email TEXT;
ALTER TABLE pos_refund_payments ADD COLUMN manual_method TEXT CHECK(manual_method IN ('cash','transfer'));
