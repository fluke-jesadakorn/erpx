-- "Request Access" inbox for locked tiles.
-- A locked tile surfaces a button that inserts a row here; an HR Manager
-- (or the configured approver) later acknowledges it.

CREATE TABLE IF NOT EXISTS access_requests (
  id                    serial PRIMARY KEY,
  actor_id              int NOT NULL REFERENCES users(id),
  tile_id               text NOT NULL,
  tile_title            text,
  note                  text,
  status                text NOT NULL DEFAULT 'pending',   -- 'pending' | 'approved' | 'denied'
  target_user_id        int REFERENCES users(id),
  target_role           text,
  created_at            timestamptz NOT NULL DEFAULT now(),
  resolved_at           timestamptz,
  resolved_by_user_id   int REFERENCES users(id),
  resolved_note         text
);

-- A user can only have one pending request per tile.
CREATE UNIQUE INDEX IF NOT EXISTS access_requests_pending_uniq
  ON access_requests (actor_id, tile_id)
  WHERE status = 'pending';

CREATE INDEX IF NOT EXISTS access_requests_status_idx
  ON access_requests (status, created_at DESC);
CREATE INDEX IF NOT EXISTS access_requests_target_idx
  ON access_requests (target_user_id)
  WHERE status = 'pending';