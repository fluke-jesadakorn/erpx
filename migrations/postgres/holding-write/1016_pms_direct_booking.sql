-- Provider attempts retain their original account, mode, money and Stay allocation.
ALTER TABLE pms_booking_payments DROP CONSTRAINT pms_booking_payments_reservation_id_key;
ALTER TABLE pms_booking_payments ADD COLUMN allocation_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE pms_booking_payments ADD COLUMN methods_json TEXT NOT NULL DEFAULT '["promptpay"]';
ALTER TABLE pms_booking_payments ADD COLUMN clearing_account TEXT;
ALTER TABLE pms_booking_payments ADD COLUMN payment_intent_id TEXT;
ALTER TABLE pms_booking_payments ADD COLUMN provider_succeeded_at BIGINT;
ALTER TABLE pms_booking_payments ADD COLUMN purpose TEXT NOT NULL DEFAULT 'reservation' CHECK(purpose IN ('reservation','change'));
ALTER TABLE pms_booking_payments ADD COLUMN proposal_id TEXT;
CREATE INDEX pms_booking_payment_reservation ON pms_booking_payments(reservation_id,created_at);
-- Existing pending attempts use their frozen, currently protected room prices.
-- Confirmed attempts derive amounts from actual receipts, never current room prices.
UPDATE pms_booking_payments p SET allocation_json=COALESCE((SELECT jsonb_agg(jsonb_build_object(
  'stayId',s.id,'folioId',s.data_json::jsonb->>'folioId','amount',
  CASE WHEN p.status='payment_pending' THEN (SELECT SUM((n->>'total')::bigint) FROM jsonb_array_elements(s.data_json::jsonb->'prices') n)
  ELSE COALESCE((SELECT SUM((e.data_json::jsonb->>'total')::bigint) FROM pms_records e WHERE e.property_id=p.property_id AND e.kind='entry' AND e.data_json::jsonb->>'folioId'=s.data_json::jsonb->>'folioId' AND e.data_json::jsonb->>'sourceKey'='online-payment:'||p.id),0) END) ORDER BY s.id)::text
  FROM pms_records s WHERE s.property_id=p.property_id AND s.kind='stay' AND s.data_json::jsonb->>'reservationId'=p.reservation_id),'[]');
UPDATE pms_booking_payments p SET clearing_account=COALESCE((SELECT accounts_json::jsonb->>'stripe' FROM pos_settings WHERE company_id=p.company_id),(SELECT config_json::jsonb->'accounts'->>'edc' FROM pms_properties WHERE id=p.property_id));

CREATE TABLE pms_booking_allocations (
 id TEXT PRIMARY KEY, payment_id TEXT NOT NULL REFERENCES pms_booking_payments(id) ON DELETE CASCADE,
 stay_id TEXT NOT NULL REFERENCES pms_records(id), folio_id TEXT NOT NULL REFERENCES pms_records(id),
 entry_id TEXT NOT NULL UNIQUE REFERENCES pms_records(id), amount_minor BIGINT NOT NULL CHECK(amount_minor>0), created_at BIGINT NOT NULL,
 UNIQUE(payment_id,stay_id)
);
INSERT INTO pms_booking_allocations(id,payment_id,stay_id,folio_id,entry_id,amount_minor,created_at)
SELECT e.id,p.id,s.id,e.data_json::jsonb->>'folioId',e.id,(e.data_json::jsonb->>'total')::bigint,e.updated_at
FROM pms_booking_payments p JOIN pms_records e ON e.property_id=p.property_id AND e.kind='entry' AND e.data_json::jsonb->>'sourceKey'='online-payment:'||p.id
JOIN pms_records s ON s.property_id=p.property_id AND s.kind='stay' AND s.data_json::jsonb->>'folioId'=e.data_json::jsonb->>'folioId'
WHERE (e.data_json::jsonb->>'total')::bigint>0 ON CONFLICT DO NOTHING;

CREATE TABLE pms_booking_refund_requests (
 id TEXT PRIMARY KEY, payment_id TEXT NOT NULL REFERENCES pms_booking_payments(id) ON DELETE CASCADE,
 amount_minor BIGINT NOT NULL CHECK(amount_minor>0), allocations_json TEXT NOT NULL,
 reason TEXT NOT NULL, requested_by TEXT NOT NULL, approved_by TEXT, provider_id TEXT UNIQUE,
 state TEXT NOT NULL CHECK(state IN ('requested','approved','processing','requires_action','succeeded','failed')),
 generation INTEGER NOT NULL DEFAULT 0,
 last_error TEXT, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
 CHECK(approved_by IS NULL OR approved_by<>requested_by)
);
CREATE INDEX pms_booking_refund_due ON pms_booking_refund_requests(state,updated_at);
CREATE TABLE pms_booking_changes (
 id TEXT PRIMARY KEY, property_id TEXT NOT NULL REFERENCES pms_properties(id), reservation_id TEXT NOT NULL REFERENCES pms_records(id),
 request_hash TEXT NOT NULL, reservation_version INTEGER NOT NULL, proposal_json TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('payment_pending','applied','expired')), expires_at BIGINT NOT NULL, created_at BIGINT NOT NULL
);
CREATE UNIQUE INDEX pms_booking_one_pending_change ON pms_booking_changes(reservation_id) WHERE state='payment_pending';
ALTER TABLE pms_booking_emails DROP CONSTRAINT pms_booking_emails_kind_check;
ALTER TABLE pms_booking_emails ADD CONSTRAINT pms_booking_emails_kind_check CHECK(kind IN ('confirmation','cancellation','access','change','refund'));
ALTER TABLE pos_stripe_events ADD COLUMN provider_created_at BIGINT;

CREATE FUNCTION pms_booking_money_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' OR (TG_TABLE_NAME='pms_booking_allocations' AND TG_OP='UPDATE') THEN RAISE EXCEPTION 'PMS immutable payment allocation'; END IF;
 IF TG_OP='UPDATE' AND (OLD.account_id,OLD.livemode,OLD.amount_minor,OLD.currency,OLD.allocation_json,OLD.methods_json,OLD.clearing_account,OLD.reservation_id,OLD.property_id)
 IS DISTINCT FROM (NEW.account_id,NEW.livemode,NEW.amount_minor,NEW.currency,NEW.allocation_json,NEW.methods_json,NEW.clearing_account,NEW.reservation_id,NEW.property_id) THEN RAISE EXCEPTION 'PMS immutable payment attempt'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER pms_booking_attempt_immutable BEFORE UPDATE ON pms_booking_payments FOR EACH ROW EXECUTE FUNCTION pms_booking_money_immutable();
-- Separate trigger to retain allocations without restricting disposable fixture cleanup.
CREATE FUNCTION pms_booking_allocation_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'PMS immutable receipt allocation'; END $$;
CREATE TRIGGER pms_booking_allocation_immutable BEFORE UPDATE ON pms_booking_allocations FOR EACH ROW EXECUTE FUNCTION pms_booking_allocation_immutable();
