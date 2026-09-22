ALTER TABLE field_service_agreements
  ADD COLUMN IF NOT EXISTS warranty_discount_bps INTEGER NOT NULL DEFAULT 0
  CHECK (warranty_discount_bps BETWEEN 0 AND 10000);

ALTER TABLE field_service_billing_releases
  ADD COLUMN IF NOT EXISTS warranty_applied BOOLEAN NOT NULL DEFAULT FALSE,
  ADD COLUMN IF NOT EXISTS warranty_discount_bps INTEGER NOT NULL DEFAULT 0
  CHECK (warranty_discount_bps BETWEEN 0 AND 10000);

CREATE OR REPLACE FUNCTION field_service_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_TABLE_NAME IN ('field_service_agreement_events','field_service_order_events','field_service_billing_releases') OR TG_OP='DELETE' THEN
  RAISE EXCEPTION 'FIELD_SERVICE_HISTORY_IMMUTABLE' USING ERRCODE='23514';
 END IF;
 IF TG_TABLE_NAME='field_service_agreements' THEN
  IF (NEW.id,NEW.company_id,NEW.branch_id,NEW.department_id,NEW.customer_id,NEW.asset_id,NEW.currency,NEW.visit_rate_minor,NEW.warranty_discount_bps,NEW.created_by,NEW.created_at)
     IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.branch_id,OLD.department_id,OLD.customer_id,OLD.asset_id,OLD.currency,OLD.visit_rate_minor,OLD.warranty_discount_bps,OLD.created_by,OLD.created_at) THEN
   RAISE EXCEPTION 'FIELD_SERVICE_AGREEMENT_SOURCE_IMMUTABLE' USING ERRCODE='23514';
  END IF;
 ELSIF TG_TABLE_NAME='field_service_orders' THEN
  IF (NEW.id,NEW.company_id,NEW.branch_id,NEW.department_id,NEW.ticket_id,NEW.ticket_version,NEW.agreement_id,NEW.agreement_version,NEW.asset_id,NEW.created_by,NEW.created_at)
     IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.branch_id,OLD.department_id,OLD.ticket_id,OLD.ticket_version,OLD.agreement_id,OLD.agreement_version,OLD.asset_id,OLD.created_by,OLD.created_at) THEN
   RAISE EXCEPTION 'FIELD_SERVICE_ORDER_SOURCE_IMMUTABLE' USING ERRCODE='23514';
  END IF;
 END IF;
 RETURN NEW;
END $$;
