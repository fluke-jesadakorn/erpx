-- Original delivery snapshots and legacy command journals are retained. An
-- existing notice acquires current authority only through its explicit retry.
ALTER TABLE commerce_customer_notifications DROP CONSTRAINT commerce_customer_notifications_status_check;
ALTER TABLE commerce_customer_notifications ADD CONSTRAINT commerce_customer_notifications_status_check
 CHECK(status IN ('pending','sending','sent','failed','cancelled','unknown'));
CREATE TABLE sales_notification_execution_authorizations (
 command_id TEXT PRIMARY KEY,
 notification_id TEXT NOT NULL REFERENCES commerce_customer_notifications(id),
 company_id TEXT NOT NULL REFERENCES organization_nodes(id),
 kind TEXT NOT NULL CHECK(kind IN ('queue','retry')),
 actor_id TEXT NOT NULL,
 assignment_id TEXT NOT NULL,
 authority_json TEXT NOT NULL CHECK(erpx_json_valid(authority_json)),
 source_hash TEXT NOT NULL CHECK(source_hash ~ '^[a-f0-9]{64}$'),
 notification_version BIGINT NOT NULL CHECK(notification_version>0),
 created_at BIGINT NOT NULL
);
CREATE INDEX sales_notification_execution_authorizations_source
 ON sales_notification_execution_authorizations(notification_id,notification_version DESC,created_at DESC,command_id);
CREATE FUNCTION sales_notification_execution_authorization_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'SALES_NOTIFICATION_EXECUTION_AUTHORIZATION_IMMUTABLE' USING ERRCODE='23514';END $$;
CREATE TRIGGER sales_notification_execution_authorization_immutable
 BEFORE UPDATE OR DELETE ON sales_notification_execution_authorizations
 FOR EACH ROW EXECUTE FUNCTION sales_notification_execution_authorization_immutable();

-- Commit a durable attempt before external I/O. A crash or uncertain response
-- becomes reconciliation work; it must not silently authorize another send.
CREATE TABLE sales_notification_execution_attempts (
 id TEXT PRIMARY KEY,
 notification_id TEXT NOT NULL REFERENCES commerce_customer_notifications(id),
 authorization_command_id TEXT NOT NULL REFERENCES sales_notification_execution_authorizations(command_id),
 company_id TEXT NOT NULL REFERENCES organization_nodes(id),
 attempt BIGINT NOT NULL CHECK(attempt>0),
 lease_until BIGINT NOT NULL,
 source_hash TEXT NOT NULL CHECK(source_hash ~ '^[a-f0-9]{64}$'),
 state TEXT NOT NULL CHECK(state IN ('sending','sent','failed','unknown','cancelled')),
 provider_message_id TEXT,
 result_code TEXT,
 created_at BIGINT NOT NULL,
 updated_at BIGINT NOT NULL,
 UNIQUE(notification_id,authorization_command_id,attempt)
);
CREATE INDEX sales_notification_execution_attempts_source
 ON sales_notification_execution_attempts(notification_id,state,created_at);
CREATE FUNCTION sales_notification_execution_attempt_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' THEN RAISE EXCEPTION 'SALES_NOTIFICATION_EXECUTION_ATTEMPT_IMMUTABLE' USING ERRCODE='23514';END IF;
 IF (NEW.id,NEW.notification_id,NEW.authorization_command_id,NEW.company_id,NEW.attempt,NEW.lease_until,NEW.source_hash,NEW.created_at)
 IS DISTINCT FROM (OLD.id,OLD.notification_id,OLD.authorization_command_id,OLD.company_id,OLD.attempt,OLD.lease_until,OLD.source_hash,OLD.created_at)
 OR OLD.state<>'sending' AND NEW IS DISTINCT FROM OLD THEN
 RAISE EXCEPTION 'SALES_NOTIFICATION_EXECUTION_ATTEMPT_IMMUTABLE' USING ERRCODE='23514';END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER sales_notification_execution_attempt_guard
 BEFORE UPDATE OR DELETE ON sales_notification_execution_attempts
 FOR EACH ROW EXECUTE FUNCTION sales_notification_execution_attempt_guard();
