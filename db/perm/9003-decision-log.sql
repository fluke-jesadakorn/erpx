-- 9003-decision-log.sql
-- perm.decision_log — every authorize() call records an allow/deny row.
-- This is distinct from perm.audit which only logs RBAC mutations.

BEGIN;

CREATE TABLE IF NOT EXISTS perm.decision_log (
  id              bigserial PRIMARY KEY,
  actor_user_id   int,
  action_kind     text NOT NULL,
  action_target   text NOT NULL,
  resource_type   text,
  resource_id     text,
  decision        text NOT NULL,
  reason          text,
  matched_perm    text,
  matched_policy  text,
  occurred_at     timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS perm_decision_log_actor_idx
  ON perm.decision_log (actor_user_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS perm_decision_log_action_idx
  ON perm.decision_log (action_kind, occurred_at DESC);
CREATE INDEX IF NOT EXISTS perm_decision_log_resource_idx
  ON perm.decision_log (resource_type, resource_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS perm_decision_log_decision_idx
  ON perm.decision_log (decision, occurred_at DESC);

COMMIT;