-- Calendar dates are independent of stay state; overdue active stays remain
-- visible through the existing inventory state index.
CREATE INDEX pms_stay_arrival_reader ON pms_records
  (property_id,(data_json::jsonb->>'arrival')) WHERE kind='stay';
CREATE INDEX pms_stay_departure_reader ON pms_records
  (property_id,(data_json::jsonb->>'departure')) WHERE kind='stay';
CREATE INDEX pms_open_account_reader ON pms_records(property_id)
  WHERE kind='guest_account' AND data_json::jsonb->>'state'!='closed';
