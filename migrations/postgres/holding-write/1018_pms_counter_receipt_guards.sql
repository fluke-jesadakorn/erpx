ALTER TABLE pms_stripe_payments ADD COLUMN clearing_account TEXT;
ALTER TABLE pms_stripe_payments ADD COLUMN provider_succeeded_at BIGINT;
ALTER TABLE pms_stripe_payments ADD COLUMN refund_reserved BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE pms_stripe_payments ADD COLUMN refund_liability_posted BOOLEAN NOT NULL DEFAULT false;
UPDATE pms_stripe_payments p SET clearing_account=COALESCE((SELECT accounts_json::jsonb->>'stripe' FROM pos_settings WHERE company_id=p.company_id),(SELECT config_json::jsonb->'accounts'->>'edc' FROM pms_properties WHERE id=p.property_id));
-- Existing unfinished refunds retain their provider id and require the new
-- independent liability review before the worker can settle them.
UPDATE pms_stripe_payments SET refund_state='requested' WHERE refund_state IN ('approved','processing');
CREATE FUNCTION pms_counter_attempt_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (OLD.account_id,OLD.livemode,OLD.amount_minor,OLD.kind,OLD.folio_id,OLD.property_id,OLD.company_id,OLD.clearing_account)
 IS DISTINCT FROM (NEW.account_id,NEW.livemode,NEW.amount_minor,NEW.kind,NEW.folio_id,NEW.property_id,NEW.company_id,NEW.clearing_account)
 THEN RAISE EXCEPTION 'PMS immutable counter receipt'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER pms_counter_attempt_immutable BEFORE UPDATE ON pms_stripe_payments FOR EACH ROW EXECUTE FUNCTION pms_counter_attempt_immutable();
