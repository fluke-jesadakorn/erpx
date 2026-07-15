-- folio/db/migrations/2026-07-12-B-waybill-watchers.sql
--
-- waybill_watchers: per-user subscription to a future stage of a waybill.
-- Inserted when an approver clicks "🔔 Notify me when this step activates"
-- on the new /waybill/{id} page. Marked notified_at=now() inside the same
-- transaction as recordEvent() when the waybill advances to that stage.
-- Inbox page (/inbox) reads both unread and notified rows.

BEGIN;

CREATE TABLE IF NOT EXISTS waybill_watchers (
  id            BIGSERIAL PRIMARY KEY,
  waybill_id    TEXT        NOT NULL REFERENCES waybills(id) ON DELETE CASCADE,
  stage_key     TEXT        NOT NULL,
  user_id       INT         NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  notified_at   TIMESTAMP,
  created_at    TIMESTAMP   NOT NULL DEFAULT now(),
  UNIQUE (waybill_id, stage_key, user_id)
);

CREATE INDEX IF NOT EXISTS idx_watchers_user_unread
  ON waybill_watchers (user_id)
  WHERE notified_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_watchers_waybill
  ON waybill_watchers (waybill_id);

INSERT INTO perm.audit (kind, target) VALUES (
  'schema.migration',
  jsonb_build_object(
    'migration', '2026-07-12-B-waybill-watchers',
    'table_created', 'waybill_watchers'
  )
);

COMMIT;