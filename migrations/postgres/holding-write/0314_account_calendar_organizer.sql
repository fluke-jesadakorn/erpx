-- Active Company accounts may organize personal meetings without an HR Person.
-- The existing composite Person reference still applies when a Person is supplied.
ALTER TABLE calendar_appointments ALTER COLUMN organizer_person_id DROP NOT NULL;
