ALTER TABLE pms_stripe_payments DROP CONSTRAINT pms_stripe_payments_refund_state_check;
ALTER TABLE pms_stripe_payments ADD CONSTRAINT pms_stripe_payments_refund_state_check CHECK(refund_state IN ('requested','approved','processing','requires_action','succeeded','failed'));
ALTER TABLE pms_stripe_payments ADD COLUMN refund_generation INTEGER NOT NULL DEFAULT 0;
CREATE TABLE pms_counter_refund_attempts (
 payment_id TEXT NOT NULL REFERENCES pms_stripe_payments(id), generation INTEGER NOT NULL,
 provider_id TEXT NOT NULL UNIQUE, amount_minor BIGINT NOT NULL CHECK(amount_minor>0),
 reason TEXT NOT NULL, requested_by TEXT NOT NULL, approved_by TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('processing','requires_action','succeeded','failed')),
 created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
 PRIMARY KEY(payment_id,generation)
);
INSERT INTO pms_counter_refund_attempts
 SELECT id,0,refund_id,refund_amount_minor,COALESCE(refund_reason,'Legacy counter refund'),COALESCE(refund_requested_by,'system:stripe'),COALESCE(refund_approved_by,'legacy:review'),
 CASE WHEN refund_state='succeeded' THEN 'succeeded' WHEN refund_state='failed' THEN 'failed' ELSE 'processing' END,created_at,updated_at
 FROM pms_stripe_payments WHERE refund_id IS NOT NULL AND refund_amount_minor>0;
CREATE FUNCTION pms_counter_refund_source_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (OLD.payment_id,OLD.generation,OLD.provider_id,OLD.amount_minor,OLD.reason,OLD.requested_by,OLD.approved_by,OLD.created_at)
 IS DISTINCT FROM (NEW.payment_id,NEW.generation,NEW.provider_id,NEW.amount_minor,NEW.reason,NEW.requested_by,NEW.approved_by,NEW.created_at)
 THEN RAISE EXCEPTION 'PMS immutable counter refund source'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER pms_counter_refund_source_immutable BEFORE UPDATE ON pms_counter_refund_attempts FOR EACH ROW EXECUTE FUNCTION pms_counter_refund_source_immutable();
INSERT INTO pms_booking_refund_attempts(request_id,generation,provider_id,created_at)
 SELECT id,generation,provider_id,updated_at FROM pms_booking_refund_requests WHERE provider_id IS NOT NULL ON CONFLICT DO NOTHING;
