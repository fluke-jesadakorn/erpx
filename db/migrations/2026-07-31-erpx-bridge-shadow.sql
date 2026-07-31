BEGIN;

-- Folio remains the command owner. This append-only queue is the durable
-- handoff for an explicitly enabled ERPX shadow publisher; it never replaces
-- Folio writes and never stores credentials or customer secrets.
CREATE TABLE IF NOT EXISTS platform.bridge_outbox (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  event_type text NOT NULL,
  aggregate_type text NOT NULL,
  aggregate_id text NOT NULL,
  aggregate_version bigint NOT NULL CHECK (aggregate_version > 0),
  idempotency_key text NOT NULL UNIQUE,
  payload jsonb NOT NULL,
  occurred_at timestamptz NOT NULL DEFAULT now(),
  available_at timestamptz NOT NULL DEFAULT now(),
  attempts integer NOT NULL DEFAULT 0 CHECK (attempts >= 0),
  claimed_at timestamptz,
  claimed_by uuid,
  published_at timestamptz,
  last_error text,
  CHECK (published_at IS NULL OR published_at >= occurred_at)
);

CREATE INDEX IF NOT EXISTS platform_bridge_outbox_ready_idx
  ON platform.bridge_outbox (available_at, occurred_at, id)
  WHERE published_at IS NULL;

CREATE TABLE IF NOT EXISTS platform.role_assignment_events (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  assignment_id uuid NOT NULL REFERENCES platform.role_assignments(id) ON DELETE RESTRICT,
  event_type text NOT NULL CHECK (event_type IN ('granted', 'renewed', 'revoked')),
  assignment_version integer NOT NULL CHECK (assignment_version > 0),
  actor text NOT NULL,
  reason text NOT NULL,
  occurred_at timestamptz NOT NULL DEFAULT now(),
  payload jsonb NOT NULL DEFAULT '{}'::jsonb,
  UNIQUE (assignment_id, event_type, assignment_version)
);

CREATE INDEX IF NOT EXISTS platform_role_assignment_events_idx
  ON platform.role_assignment_events (assignment_id, occurred_at, id);

COMMIT;
