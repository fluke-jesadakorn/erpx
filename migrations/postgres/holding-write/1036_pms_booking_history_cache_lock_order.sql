-- A stay transition also changes reservation history. History-only updates take
-- the pms_records cache revision, while observation-producing updates previously
-- took pms_booking_observations first. Record order varies between bookings, so
-- concurrent properties could acquire those two cache rows in opposite order.
-- PostgreSQL fires same-event row triggers alphabetically. Invalidate the owner
-- record before appending its observation so both paths acquire owner then
-- projection. Keep the functions, append-only evidence, transactional revisions
-- and Property/IAM locks unchanged; no data or counters are rewritten.
-- https://www.postgresql.org/docs/18/trigger-definition.html
ALTER TRIGGER pms_capture_booking_observation ON pms_records
  RENAME TO work_cache_pms_booking_observation;
