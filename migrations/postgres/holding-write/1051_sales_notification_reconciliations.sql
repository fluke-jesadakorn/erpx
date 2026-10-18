-- Preserve the immutable uncertain attempt. Resolution is a separate normal
-- independently reviewed command backed by a read-only provider observation.
CREATE TABLE sales_notification_reconciliations (
 id TEXT PRIMARY KEY,
 command_id TEXT NOT NULL UNIQUE REFERENCES command_receipts(command_id) DEFERRABLE INITIALLY DEFERRED,
 notification_id TEXT NOT NULL REFERENCES commerce_customer_notifications(id),
 attempt_id TEXT NOT NULL UNIQUE REFERENCES sales_notification_execution_attempts(id),
 company_id TEXT NOT NULL REFERENCES organization_nodes(id),
 actor_id TEXT NOT NULL,
 assignment_id TEXT NOT NULL,
 expected_version BIGINT NOT NULL CHECK(expected_version>0),
 notification_version BIGINT NOT NULL CHECK(notification_version=expected_version+1),
 source_hash TEXT NOT NULL CHECK(source_hash ~ '^[a-f0-9]{64}$'),
 provider_message_id TEXT NOT NULL CHECK(length(provider_message_id)>0),
 provider_event TEXT NOT NULL CHECK(provider_event IN ('sent','delivered','opened','clicked','bounced','canceled','failed','suppressed')),
 outcome TEXT NOT NULL CHECK(outcome IN ('sent','cancelled')),
 provider_observation_json TEXT NOT NULL CHECK(erpx_json_valid(provider_observation_json)),
 provider_observed_at BIGINT NOT NULL,
 reason TEXT NOT NULL CHECK(length(trim(reason))>=5),
 created_at BIGINT NOT NULL
);
CREATE INDEX sales_notification_reconciliations_source ON sales_notification_reconciliations(company_id,notification_id);
CREATE FUNCTION sales_notification_reconciliation_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'SALES_NOTIFICATION_RECONCILIATION_IMMUTABLE' USING ERRCODE='23514';END $$;
CREATE TRIGGER sales_notification_reconciliation_immutable BEFORE UPDATE OR DELETE ON sales_notification_reconciliations FOR EACH ROW EXECUTE FUNCTION sales_notification_reconciliation_immutable();
