-- Folio — add `users.hired_at` for tenure-based promotion gating.
--
-- Used by the "My Progress" page to compute months-since-hire against the
-- per-level tenure requirement (L3 = 6mo, L2 = 12mo, L1 = 24mo). The
-- PromotionPreview panel reads it via `loadPromotionProgress`.
--
-- Backfill: when hired_at is unknown, use users.created_at as a proxy so the
-- column is never NULL on active users. Admins can fix it later.

BEGIN;

ALTER TABLE users
  ADD COLUMN IF NOT EXISTS hired_at date;

UPDATE users
   SET hired_at = date_trunc('day', created_at)::date
 WHERE hired_at IS NULL;

ALTER TABLE users
  ALTER COLUMN hired_at SET NOT NULL;

CREATE INDEX IF NOT EXISTS idx_users_hired_at ON users (hired_at);

COMMIT;