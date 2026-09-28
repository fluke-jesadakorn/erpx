-- Operational live count for display only. Monthly rating continues to use
-- billing_seat_usage intervals with measured coverage; this point-in-time
-- number must never be used to infer a historical seat-month.
ALTER TABLE billing_usage_snapshots
  ADD COLUMN active_seat_count INTEGER CHECK(active_seat_count>=0),
  ADD COLUMN active_seat_observed_at BIGINT CHECK(active_seat_observed_at>0),
  ADD CONSTRAINT billing_active_seat_snapshot_complete CHECK(
    (active_seat_count IS NULL)=(active_seat_observed_at IS NULL));
