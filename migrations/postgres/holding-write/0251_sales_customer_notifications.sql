-- Consent-aware customer delivery notifications with durable provider state.
ALTER TABLE finance_parties ADD COLUMN notification_email TEXT;
ALTER TABLE finance_parties ADD COLUMN notification_consent TEXT NOT NULL DEFAULT 'unknown' CHECK(notification_consent IN ('unknown','granted','revoked'));

CREATE TABLE commerce_customer_notification_settings (
 company_id TEXT PRIMARY KEY,
 adapter TEXT NOT NULL DEFAULT 'disabled' CHECK(adapter IN ('disabled','sandbox','resend')),
 version BIGINT NOT NULL DEFAULT 1,
 updated_by TEXT NOT NULL,
 updated_at BIGINT NOT NULL
);

CREATE TABLE commerce_customer_notifications (
 id TEXT PRIMARY KEY,
 company_id TEXT NOT NULL,
 branch_id TEXT,
 order_id TEXT NOT NULL REFERENCES commerce_orders(id),
 fulfillment_id TEXT NOT NULL REFERENCES commerce_fulfillments(id),
 party_id TEXT NOT NULL REFERENCES finance_parties(id),
 party_version BIGINT NOT NULL,
 recipient_email TEXT NOT NULL,
 subject TEXT NOT NULL,
 body_text TEXT NOT NULL,
 adapter TEXT NOT NULL CHECK(adapter IN ('sandbox','resend')),
 status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','sending','sent','failed','cancelled')),
 version BIGINT NOT NULL DEFAULT 1,
 attempts BIGINT NOT NULL DEFAULT 0,
 next_attempt_at BIGINT NOT NULL,
 lease_until BIGINT,
 provider_message_id TEXT,
 sent_at BIGINT,
 last_error TEXT,
 created_by TEXT NOT NULL,
 created_at BIGINT NOT NULL,
 updated_at BIGINT NOT NULL,
 UNIQUE(fulfillment_id)
);
CREATE INDEX commerce_customer_notifications_queue ON commerce_customer_notifications(status,next_attempt_at);

CREATE FUNCTION commerce_customer_notification_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF (NEW.company_id,NEW.branch_id,NEW.order_id,NEW.fulfillment_id,NEW.party_id,NEW.party_version,NEW.recipient_email,NEW.subject,NEW.body_text,NEW.adapter,NEW.created_by,NEW.created_at)
    IS DISTINCT FROM
    (OLD.company_id,OLD.branch_id,OLD.order_id,OLD.fulfillment_id,OLD.party_id,OLD.party_version,OLD.recipient_email,OLD.subject,OLD.body_text,OLD.adapter,OLD.created_by,OLD.created_at)
 THEN RAISE EXCEPTION 'CUSTOMER_NOTIFICATION_SNAPSHOT_IMMUTABLE' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER commerce_customer_notification_guard BEFORE UPDATE ON commerce_customer_notifications FOR EACH ROW EXECUTE FUNCTION commerce_customer_notification_guard();
CREATE FUNCTION commerce_customer_notification_delete_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'CUSTOMER_NOTIFICATION_HISTORY_IMMUTABLE' USING ERRCODE='23514'; END $$;
CREATE TRIGGER commerce_customer_notification_delete_guard BEFORE DELETE ON commerce_customer_notifications FOR EACH ROW EXECUTE FUNCTION commerce_customer_notification_delete_guard();

INSERT INTO work_cache_revisions(table_name) VALUES('commerce_customer_notifications'),('commerce_customer_notification_settings') ON CONFLICT DO NOTHING;
CREATE TRIGGER commerce_customer_notifications_cache AFTER INSERT OR UPDATE OR DELETE ON commerce_customer_notifications FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
CREATE TRIGGER commerce_customer_notification_settings_cache AFTER INSERT OR UPDATE OR DELETE ON commerce_customer_notification_settings FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
