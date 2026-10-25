-- Archived reservations must still prevent reusing a channel reference.
-- Nonunique: retain all legacy IDs and let the authoritative command reject
-- duplicates rather than rewriting existing data during migration.
CREATE INDEX pms_reservation_reference_reader ON pms_records
  (property_id,(data_json::jsonb->>'source'),(data_json::jsonb->>'externalRef'))
  WHERE kind='reservation';
