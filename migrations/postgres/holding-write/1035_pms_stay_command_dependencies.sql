-- Additive authoritative front-desk dependency lookups. No IDs, financial
-- snapshots or existing record data are rewritten or inferred.
CREATE INDEX pms_stay_booking_reader ON pms_records
  (property_id,(data_json::jsonb->>'reservationId')) WHERE kind='stay';
CREATE INDEX pms_account_booking_reader ON pms_records
  (property_id,(data_json::jsonb->>'reservationId')) WHERE kind='guest_account';
CREATE INDEX pms_credit_exposure_reader ON pms_records
  (property_id,(data_json::jsonb->>'partyId'))
  WHERE kind='guest_account' AND data_json::jsonb->>'state'='credit';
CREATE INDEX pms_task_room_reader ON pms_records
  (property_id,(data_json::jsonb->>'roomId')) WHERE kind='task';
