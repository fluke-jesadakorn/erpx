-- Source identity and original reversals are hotel-wide invariants, even when
-- the authoritative POS snapshot selects only complete affected accounts.
CREATE INDEX pms_entry_source_key_reader ON pms_records
  (property_id,(data_json::jsonb->>'sourceKey')) WHERE kind='entry';
CREATE INDEX pms_entry_original_id_reader ON pms_records
  (property_id,(data_json::jsonb->>'originalId')) WHERE kind='entry';
