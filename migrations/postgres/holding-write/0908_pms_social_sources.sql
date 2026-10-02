ALTER TABLE social_post_sources DROP CONSTRAINT social_post_sources_source_kind_check;
ALTER TABLE social_post_sources ADD CONSTRAINT social_post_sources_source_kind_check
  CHECK (source_kind IN ('staff_task','staff_project','pms_record'));

-- Delivery receipts belong to the existing outbox; posts remain in Social.
CREATE TABLE pms_social_event_receipts (
  event_id TEXT PRIMARY KEY REFERENCES outbox_events(id),
  post_id TEXT NOT NULL REFERENCES social_posts(id),
  created_at BIGINT NOT NULL
);
DO $$
DECLARE app_role text := current_database() || '_app'; jobs_role text := current_database() || '_jobs';
BEGIN
  IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname=app_role) THEN
    EXECUTE format('GRANT SELECT ON pms_social_event_receipts TO %I',app_role);
  END IF;
  IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname=jobs_role) THEN
    EXECUTE format('GRANT SELECT,INSERT ON pms_social_event_receipts TO %I',jobs_role);
  END IF;
END $$;
