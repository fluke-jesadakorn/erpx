CREATE TABLE IF NOT EXISTS folio.policy_lint_results (
  policy_id text NOT NULL REFERENCES perm.policies(id) ON DELETE CASCADE,
  findings jsonb NOT NULL DEFAULT '[]'::jsonb,
  generated_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (policy_id)
);

CREATE INDEX IF NOT EXISTS folio_policy_lint_results_generated_at_idx
  ON folio.policy_lint_results(generated_at DESC);