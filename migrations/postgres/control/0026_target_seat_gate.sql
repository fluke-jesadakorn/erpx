-- Seat entitlement and pricing are under review. The existing target version
-- remains provisional; no target invoice may be approved while terms or the
-- Holding's seat meter are unavailable. A future decision must add a new
-- immutable price version and a measured seat projection.
ALTER TABLE billing_target_prices
  ADD COLUMN seat_terms_status TEXT NOT NULL DEFAULT 'undefined'
    CHECK (seat_terms_status IN ('undefined','defined')),
  ADD COLUMN included_seats INTEGER CHECK (included_seats IS NULL OR included_seats >= 0),
  ADD COLUMN seat_monthly_satang BIGINT CHECK (seat_monthly_satang IS NULL OR seat_monthly_satang >= 0),
  ADD COLUMN storage_allowance_mode TEXT NOT NULL DEFAULT 'holding_flat'
    CHECK (storage_allowance_mode IN ('holding_flat','per_paid_seat_time')),
  ADD COLUMN rating_enabled BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE billing_usage_snapshots ADD COLUMN seat_coverage TEXT NOT NULL DEFAULT 'unavailable'
  CHECK (seat_coverage IN ('unavailable','measured'));
ALTER TABLE billing_usage_snapshots ADD COLUMN seat_source_count INTEGER NOT NULL DEFAULT 0
  CHECK (seat_source_count >= 0);

INSERT INTO billing_target_prices
  (id,effective_month,base_monthly_satang,included_original_bytes,
   storage_satang_per_gb_month,ai_markup_percent,seat_terms_status,
   included_seats,seat_monthly_satang,created_at,created_by)
VALUES
  ('holding-usage-2026-11-v2','2026-11',149000,20000000000,500,10,
   'defined',0,19900,0,'system');

-- V1/V2 remain immutable provisional history. V3 is the only rateable
-- Sandbox hypothesis; it still cannot invoice until seat coverage and all
-- other meter/cost gates are complete.
INSERT INTO billing_target_prices
  (id,effective_month,base_monthly_satang,included_original_bytes,
   storage_satang_per_gb_month,ai_markup_percent,seat_terms_status,
   included_seats,seat_monthly_satang,storage_allowance_mode,rating_enabled,
   created_at,created_by)
VALUES
  ('holding-usage-2026-12-v3','2026-12',0,2000000000,500,10,
   'defined',0,49900,'per_paid_seat_time',TRUE,0,'system');

-- One opaque row per distinct Holding human. Intervals are canonical disjoint
-- [start,end] UTC milliseconds, already clipped to the Bangkok calendar month.
-- This never contains user IDs, names, emails or assignment business records.
CREATE TABLE billing_seat_usage (
  holding_id TEXT NOT NULL,
  month TEXT NOT NULL,
  user_key TEXT NOT NULL CHECK (user_key ~ '^[a-f0-9]{64}$'),
  intervals_json JSONB NOT NULL CHECK (jsonb_typeof(intervals_json)='array'),
  snapshot_id TEXT NOT NULL,
  PRIMARY KEY (holding_id,month,user_key),
  FOREIGN KEY (holding_id,month) REFERENCES billing_usage_snapshots(holding_id,month)
    ON DELETE CASCADE
);
CREATE INDEX billing_seat_usage_month ON billing_seat_usage(month,holding_id);

-- Control principals' mutable status is captured from this migration onward.
-- A baseline at migration time is not fabricated as historical coverage.
CREATE TABLE billing_principal_status_coverage (
  singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
  since_at BIGINT NOT NULL
);
INSERT INTO billing_principal_status_coverage(singleton,since_at)
VALUES(TRUE,(EXTRACT(EPOCH FROM clock_timestamp())*1000)::BIGINT);
CREATE TABLE billing_principal_status_events (
  id BIGSERIAL PRIMARY KEY,
  user_id TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('active','suspended','removed')),
  effective_at BIGINT NOT NULL
);
INSERT INTO billing_principal_status_events(user_id,status,effective_at)
SELECT user_id,status,(SELECT since_at FROM billing_principal_status_coverage)
FROM principals;
CREATE INDEX billing_principal_status_timeline
  ON billing_principal_status_events(user_id,effective_at,id);
CREATE FUNCTION capture_billing_principal_status() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
  changed_at BIGINT := (EXTRACT(EPOCH FROM clock_timestamp())*1000)::BIGINT;
BEGIN
  IF TG_OP='DELETE' THEN
    INSERT INTO billing_principal_status_events(user_id,status,effective_at)
    VALUES(OLD.user_id,'removed',changed_at);
    RETURN OLD;
  END IF;
  IF TG_OP='INSERT' THEN
    INSERT INTO billing_principal_status_events(user_id,status,effective_at)
    VALUES(NEW.user_id,NEW.status,changed_at);
  ELSIF NEW.user_id IS DISTINCT FROM OLD.user_id THEN
    INSERT INTO billing_principal_status_events(user_id,status,effective_at)
    VALUES(OLD.user_id,'removed',changed_at),(NEW.user_id,NEW.status,changed_at);
  ELSIF NEW.status IS DISTINCT FROM OLD.status THEN
    INSERT INTO billing_principal_status_events(user_id,status,effective_at)
    VALUES(NEW.user_id,NEW.status,changed_at);
  END IF;
  RETURN NEW;
END;
$$;
CREATE TRIGGER billing_principal_status_changes
  AFTER INSERT OR UPDATE OR DELETE ON principals
  FOR EACH ROW EXECUTE FUNCTION capture_billing_principal_status();
CREATE TRIGGER billing_principal_event_immutable BEFORE UPDATE OR DELETE ON billing_principal_status_events
  FOR EACH ROW EXECUTE FUNCTION billing_target_immutable_row();
CREATE TRIGGER billing_principal_coverage_immutable BEFORE UPDATE OR DELETE ON billing_principal_status_coverage
  FOR EACH ROW EXECUTE FUNCTION billing_target_immutable_row();
