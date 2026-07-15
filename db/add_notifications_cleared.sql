-- Add cleared_at column for soft-delete on per-user inbox.
-- Audit log remains in domain_events (immutable); this only marks inbox rows as cleared.
--
-- Run with:
--   PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -f db/add_notifications_cleared.sql

ALTER TABLE notifications
  ADD COLUMN IF NOT EXISTS cleared_at TIMESTAMPTZ NULL;

CREATE INDEX IF NOT EXISTS idx_notif_user_cleared
  ON notifications (user_id, cleared_at)
  WHERE cleared_at IS NULL;