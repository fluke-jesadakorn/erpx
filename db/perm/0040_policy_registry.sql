-- perm.policies — code-first policy registry (Phase 2 will seed).
CREATE TABLE perm.policies (
  id          text PRIMARY KEY,
  name        text NOT NULL UNIQUE,
  ast         jsonb NOT NULL,
  description text,
  enabled     boolean NOT NULL DEFAULT true,
  version     integer NOT NULL DEFAULT 1,
  created_at  timestamptz NOT NULL DEFAULT now(),
  updated_at  timestamptz NOT NULL DEFAULT now()
);

-- perm.policy_decisions — audit log of every policy evaluation.
CREATE TABLE perm.policy_decisions (
  id          bigserial PRIMARY KEY,
  actor_id    integer,
  policy_id   text,
  surface     text NOT NULL,
  target      text,
  decision    text NOT NULL,
  reasons     jsonb,
  resource    jsonb,
  occurred_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX policy_decisions_actor_idx ON perm.policy_decisions (actor_id, occurred_at DESC);
CREATE INDEX policy_decisions_policy_idx ON perm.policy_decisions (policy_id, occurred_at DESC);