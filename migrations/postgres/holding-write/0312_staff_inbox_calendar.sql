-- A logical event has one delivery per recipient. Legacy rows remain writable while
-- domain producers move to the typed publisher.
CREATE TABLE inbox_events (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, event_key TEXT NOT NULL,
 source_type TEXT NOT NULL, source_id TEXT NOT NULL, source_version BIGINT NOT NULL,
 actor_user_id TEXT, category TEXT NOT NULL CHECK(category IN ('work','calendar','mention','notice','system')),
 preview TEXT NOT NULL, created_at BIGINT NOT NULL,
 UNIQUE(company_id,event_key)
);
CREATE TABLE inbox_deliveries (
 id TEXT PRIMARY KEY, event_id TEXT NOT NULL REFERENCES inbox_events(id),
 company_id TEXT NOT NULL, user_id TEXT NOT NULL, delivered_at BIGINT NOT NULL,
 read_at BIGINT, archived_at BIGINT, UNIQUE(event_id,user_id)
);
CREATE INDEX inbox_deliveries_user ON inbox_deliveries(company_id,user_id,delivered_at DESC,id DESC);
CREATE TABLE inbox_notices (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, sender_user_id TEXT NOT NULL,
 title TEXT NOT NULL CHECK(char_length(title) BETWEEN 1 AND 160),
 body TEXT NOT NULL CHECK(char_length(body) BETWEEN 1 AND 5000),
 audience_json JSONB NOT NULL, recipient_count INTEGER NOT NULL,
 created_at BIGINT NOT NULL
);
CREATE TABLE inbox_notice_targets (
 notice_id TEXT NOT NULL REFERENCES inbox_notices(id), company_id TEXT NOT NULL,
 user_id TEXT NOT NULL, delivered_at BIGINT, PRIMARY KEY(notice_id,user_id)
);
CREATE INDEX inbox_notice_targets_pending ON inbox_notice_targets(notice_id,user_id) WHERE delivered_at IS NULL;

-- Each legacy delivery is a logical event of one. This preserves existing IDs
-- and read state without assuming that identical text means one source event.
INSERT INTO inbox_events(id,company_id,event_key,source_type,source_id,source_version,category,preview,created_at)
 SELECT 'legacy:'||id,company_id,'legacy:'||id,record_type,record_id,record_version,
 CASE WHEN record_type='post' THEN 'mention' ELSE 'work' END,message,created_at
 FROM staff_notifications ON CONFLICT DO NOTHING;
INSERT INTO inbox_deliveries(id,event_id,company_id,user_id,delivered_at,read_at)
 SELECT id,'legacy:'||id,company_id,user_id,created_at,read_at FROM staff_notifications ON CONFLICT DO NOTHING;
CREATE FUNCTION inbox_bridge_legacy() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='INSERT' THEN
  INSERT INTO inbox_events(id,company_id,event_key,source_type,source_id,source_version,category,preview,created_at)
   VALUES('legacy:'||NEW.id,NEW.company_id,'legacy:'||NEW.id,NEW.record_type,NEW.record_id,NEW.record_version,
    CASE WHEN NEW.record_type='post' THEN 'mention' ELSE 'work' END,NEW.message,NEW.created_at)
   ON CONFLICT DO NOTHING;
  INSERT INTO inbox_deliveries(id,event_id,company_id,user_id,delivered_at,read_at)
   VALUES(NEW.id,'legacy:'||NEW.id,NEW.company_id,NEW.user_id,NEW.created_at,NEW.read_at)
   ON CONFLICT DO NOTHING;
 ELSIF NEW.read_at IS DISTINCT FROM OLD.read_at THEN
  UPDATE inbox_deliveries SET read_at=NEW.read_at WHERE id=NEW.id AND company_id=NEW.company_id AND user_id=NEW.user_id;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER inbox_legacy_insert AFTER INSERT ON staff_notifications FOR EACH ROW EXECUTE FUNCTION inbox_bridge_legacy();
CREATE TRIGGER inbox_legacy_read AFTER UPDATE OF read_at ON staff_notifications FOR EACH ROW EXECUTE FUNCTION inbox_bridge_legacy();

-- Meeting invitations identify accounts, including active accounts without an
-- HR Person. Existing Person links remain for roster and absence calculations.
CREATE TABLE calendar_attendees (
 company_id TEXT NOT NULL, appointment_id TEXT NOT NULL, user_id TEXT NOT NULL,
 person_id TEXT, attendance TEXT NOT NULL DEFAULT 'required' CHECK(attendance IN ('organizer','required','optional')),
 response TEXT NOT NULL DEFAULT 'pending' CHECK(response IN ('pending','accepted','tentative','declined')),
 reminder_minutes INTEGER CHECK(reminder_minutes BETWEEN 0 AND 10080),
 response_at BIGINT, PRIMARY KEY(appointment_id,user_id),
 FOREIGN KEY(company_id,appointment_id) REFERENCES calendar_appointments(company_id,id)
);
CREATE INDEX calendar_attendees_user ON calendar_attendees(company_id,user_id,appointment_id);
INSERT INTO calendar_attendees(company_id,appointment_id,user_id,person_id,attendance,response,reminder_minutes)
 SELECT p.company_id,p.appointment_id,h.user_id,p.person_id,
  CASE WHEN a.organizer_person_id=p.person_id THEN 'organizer' ELSE 'required' END,
  CASE WHEN a.organizer_person_id=p.person_id THEN 'accepted' ELSE 'pending' END,15
 FROM calendar_appointment_participants p JOIN calendar_appointments a ON a.id=p.appointment_id AND a.company_id=p.company_id
 JOIN hr_people h ON h.id=p.person_id AND h.company_id=p.company_id WHERE h.user_id IS NOT NULL
 ON CONFLICT DO NOTHING;

ALTER TABLE calendar_appointments ADD COLUMN all_day BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE calendar_appointments ADD COLUMN start_date TEXT;
ALTER TABLE calendar_appointments ADD COLUMN end_date TEXT;
ALTER TABLE calendar_appointments ADD CONSTRAINT calendar_all_day_dates
 CHECK((NOT all_day AND start_date IS NULL AND end_date IS NULL) OR
       (all_day AND start_date IS NOT NULL AND end_date IS NOT NULL AND end_date>start_date));

CREATE TABLE calendar_series (
 appointment_id TEXT PRIMARY KEY REFERENCES calendar_appointments(id), company_id TEXT NOT NULL,
 frequency TEXT NOT NULL CHECK(frequency IN ('daily','weekly','monthly','yearly')),
 interval_count INTEGER NOT NULL CHECK(interval_count BETWEEN 1 AND 99),
 weekdays TEXT NOT NULL DEFAULT '', end_kind TEXT NOT NULL CHECK(end_kind IN ('never','count','until')),
 occurrence_count INTEGER, until_date TEXT, version INTEGER NOT NULL DEFAULT 1,
 CHECK((end_kind='count' AND occurrence_count BETWEEN 1 AND 1000) OR
       (end_kind='until' AND until_date IS NOT NULL) OR end_kind='never')
);
CREATE TABLE calendar_occurrence_exceptions (
 company_id TEXT NOT NULL, appointment_id TEXT NOT NULL REFERENCES calendar_appointments(id),
 occurrence_key TEXT NOT NULL, start_local TEXT, end_local TEXT, cancelled BOOLEAN NOT NULL DEFAULT false,
 version INTEGER NOT NULL DEFAULT 1, PRIMARY KEY(appointment_id,occurrence_key)
);
CREATE TABLE calendar_occurrence_responses (
 company_id TEXT NOT NULL, appointment_id TEXT NOT NULL, occurrence_key TEXT NOT NULL,
 user_id TEXT NOT NULL, response TEXT NOT NULL CHECK(response IN ('accepted','tentative','declined')),
 responded_at BIGINT NOT NULL, PRIMARY KEY(appointment_id,occurrence_key,user_id)
);
CREATE TABLE calendar_reminder_deliveries (
 company_id TEXT NOT NULL, appointment_id TEXT NOT NULL, occurrence_key TEXT NOT NULL,
 user_id TEXT NOT NULL, due_at BIGINT NOT NULL, delivered_at BIGINT,
 PRIMARY KEY(appointment_id,occurrence_key,user_id)
);
CREATE INDEX calendar_reminders_due ON calendar_reminder_deliveries(due_at) WHERE delivered_at IS NULL;
