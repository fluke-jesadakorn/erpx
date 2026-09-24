ALTER TABLE pms_stripe_payments ADD COLUMN refund_state TEXT CHECK(refund_state IN ('requested','approved','processing','succeeded','failed'));
ALTER TABLE pms_stripe_payments ADD COLUMN refund_amount_minor BIGINT;
ALTER TABLE pms_stripe_payments ADD COLUMN refund_reason TEXT;
ALTER TABLE pms_stripe_payments ADD COLUMN refund_requested_by TEXT;
ALTER TABLE pms_stripe_payments ADD COLUMN refund_approved_by TEXT;
ALTER TABLE pms_stripe_payments ADD COLUMN refund_id TEXT UNIQUE;
