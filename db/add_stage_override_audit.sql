-- Track CEO / admin overrides of the natural approval stage.
-- Each row is created when an out-of-stage approval still proceeds.

CREATE TABLE IF NOT EXISTS stage_override_audit (
  id              serial PRIMARY KEY,
  actor_id        int NOT NULL REFERENCES users(id),
  entity_type     text NOT NULL,         -- 'expense' | 'pr' | 'po'
  entity_id       int NOT NULL,
  attempted_stage text NOT NULL,         -- e.g. 'supervisor_review'
  required_role   text NOT NULL,         -- e.g. 'supervisor'
  actor_role      text NOT NULL,         -- e.g. 'ceo' or 'admin'
  reason          text,
  created_at      timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS stage_override_audit_entity_idx
  ON stage_override_audit (entity_type, entity_id);
CREATE INDEX IF NOT EXISTS stage_override_audit_actor_idx
  ON stage_override_audit (actor_id);