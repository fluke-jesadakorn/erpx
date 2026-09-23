CREATE TABLE company_feature_rollouts (
  company_id TEXT NOT NULL,
  module_id TEXT NOT NULL,
  rollout_state TEXT NOT NULL CHECK (rollout_state IN ('off','pilot','available')),
  version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
  reason TEXT NOT NULL,
  changed_by TEXT NOT NULL,
  changed_at BIGINT NOT NULL,
  PRIMARY KEY (company_id,module_id)
);
CREATE INDEX company_feature_rollouts_state ON company_feature_rollouts(company_id,rollout_state);
