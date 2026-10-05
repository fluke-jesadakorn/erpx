-- A retry never overwrites the provider identity of an earlier refund attempt.
CREATE TABLE pms_booking_refund_attempts (
 request_id TEXT NOT NULL REFERENCES pms_booking_refund_requests(id) ON DELETE CASCADE,
 generation INTEGER NOT NULL, provider_id TEXT NOT NULL UNIQUE, created_at BIGINT NOT NULL,
 PRIMARY KEY(request_id,generation)
);
INSERT INTO pms_booking_refund_attempts SELECT id,generation,provider_id,updated_at
 FROM pms_booking_refund_requests WHERE provider_id IS NOT NULL;
ALTER TABLE pms_booking_refund_requests ADD COLUMN legacy_review BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE pms_booking_refund_requests ADD COLUMN legacy_approved_by TEXT;
-- Preserve legacy full-refund sources and provider ids. Unfinished legacy refunds
-- require Finance to reserve their original funds and accept the liability mapping.
INSERT INTO pms_booking_refund_requests(id,payment_id,amount_minor,allocations_json,reason,requested_by,
 provider_id,state,created_at,updated_at,legacy_review,legacy_approved_by)
 SELECT 'legacy:'||p.id,p.id,p.amount_minor,
 COALESCE((SELECT jsonb_agg(jsonb_build_object('stayId',a.stay_id,'folioId',a.folio_id,'entryId',a.entry_id,'amount',a.amount_minor) ORDER BY a.stay_id)::text FROM pms_booking_allocations a WHERE a.payment_id=p.id),'[]'),
 'Legacy online cancellation refund','guest:self-service',p.refund_id,
 CASE WHEN p.refund_state='succeeded' THEN 'succeeded' ELSE 'requested' END,
 COALESCE(p.refund_requested_at,p.created_at),p.updated_at,p.refund_state<>'succeeded',p.refund_approved_by
 FROM pms_booking_payments p WHERE p.refund_state IS NOT NULL
 AND NOT EXISTS(SELECT 1 FROM pms_booking_refund_requests r WHERE r.payment_id=p.id);
CREATE FUNCTION pms_booking_refund_source_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (OLD.payment_id,OLD.amount_minor,OLD.allocations_json,OLD.reason,OLD.requested_by,OLD.created_at)
 IS DISTINCT FROM (NEW.payment_id,NEW.amount_minor,NEW.allocations_json,NEW.reason,NEW.requested_by,NEW.created_at)
 THEN RAISE EXCEPTION 'PMS immutable refund source'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER pms_booking_refund_source_immutable BEFORE UPDATE ON pms_booking_refund_requests
 FOR EACH ROW EXECUTE FUNCTION pms_booking_refund_source_immutable();
CREATE TRIGGER pms_booking_refund_attempt_immutable BEFORE UPDATE ON pms_booking_refund_attempts
 FOR EACH ROW EXECUTE FUNCTION pms_booking_allocation_immutable();
