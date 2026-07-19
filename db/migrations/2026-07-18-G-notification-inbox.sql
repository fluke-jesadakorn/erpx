ALTER TABLE folio.notifications
  ADD COLUMN IF NOT EXISTS waybill_id text,
  ADD COLUMN IF NOT EXISTS event_id uuid,
  ADD COLUMN IF NOT EXISTS category text NOT NULL DEFAULT 'update',
  ADD COLUMN IF NOT EXISTS audience text NOT NULL DEFAULT 'owner',
  ADD COLUMN IF NOT EXISTS stage_key text,
  ADD COLUMN IF NOT EXISTS message_key text,
  ADD COLUMN IF NOT EXISTS severity text NOT NULL DEFAULT 'info',
  ADD COLUMN IF NOT EXISTS href text,
  ADD COLUMN IF NOT EXISTS resolved_at timestamptz,
  ADD COLUMN IF NOT EXISTS resolved_by integer,
  ADD COLUMN IF NOT EXISTS first_opened_at timestamptz,
  ADD COLUMN IF NOT EXISTS last_opened_at timestamptz,
  ADD COLUMN IF NOT EXISTS open_count integer NOT NULL DEFAULT 0;

DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'notifications_category_check') THEN
    ALTER TABLE folio.notifications ADD CONSTRAINT notifications_category_check CHECK (category IN ('action', 'update'));
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'notifications_audience_check') THEN
    ALTER TABLE folio.notifications ADD CONSTRAINT notifications_audience_check CHECK (audience IN ('owner', 'approver', 'watcher'));
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'notifications_severity_check') THEN
    ALTER TABLE folio.notifications ADD CONSTRAINT notifications_severity_check CHECK (severity IN ('info', 'success', 'warning', 'error'));
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'notifications_open_count_check') THEN
    ALTER TABLE folio.notifications ADD CONSTRAINT notifications_open_count_check CHECK (open_count >= 0);
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'notifications_waybill_id_fkey') THEN
    ALTER TABLE folio.notifications ADD CONSTRAINT notifications_waybill_id_fkey FOREIGN KEY (waybill_id) REFERENCES folio.waybills(id) ON DELETE CASCADE;
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'notifications_event_id_fkey') THEN
    ALTER TABLE folio.notifications ADD CONSTRAINT notifications_event_id_fkey FOREIGN KEY (event_id) REFERENCES folio.waybill_events(id) ON DELETE SET NULL;
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'notifications_resolved_by_fkey') THEN
    ALTER TABLE folio.notifications ADD CONSTRAINT notifications_resolved_by_fkey FOREIGN KEY (resolved_by) REFERENCES folio.users(id) ON DELETE SET NULL;
  END IF;
END $$;

CREATE UNIQUE INDEX IF NOT EXISTS notifications_event_user_key
  ON folio.notifications(event_id, user_id, message_key);

DROP INDEX IF EXISTS folio.idx_notif_user_cleared;
CREATE INDEX IF NOT EXISTS idx_notif_user_unread
  ON folio.notifications(user_id, read_at, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_notif_user_feed
  ON folio.notifications(user_id, category, resolved_at, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_notif_waybill_stage
  ON folio.notifications(waybill_id, stage_key, category, resolved_at);
