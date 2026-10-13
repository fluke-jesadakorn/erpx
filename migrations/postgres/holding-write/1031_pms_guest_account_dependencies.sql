-- Scoped stay/entry dependencies of a paged Guest Account, without scanning
-- every closed account's historical stay or changing the page's authority.
CREATE INDEX pms_stay_guest_account_reader ON pms_records
  (property_id,(data_json::jsonb->>'guestAccountId')) WHERE kind='stay';
CREATE INDEX pms_entry_guest_account_reader ON pms_records
  (property_id,kind,(data_json::jsonb->>'guestAccountId'))
  WHERE kind IN ('entry','refund','correction');
