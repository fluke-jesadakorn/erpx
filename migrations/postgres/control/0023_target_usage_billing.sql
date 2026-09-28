-- The target offering is opt-in and Sandbox-only. Existing Support enrollments,
-- invoices and revisions remain untouched. No Holding is assigned by migration.
CREATE TABLE billing_target_prices (
  id TEXT PRIMARY KEY,
  effective_month TEXT NOT NULL CHECK (effective_month ~ '^[0-9]{4}-(0[1-9]|1[0-2])$'),
  base_monthly_satang BIGINT NOT NULL CHECK (base_monthly_satang >= 0),
  included_original_bytes BIGINT NOT NULL CHECK (included_original_bytes >= 0),
  storage_satang_per_gb_month BIGINT NOT NULL CHECK (storage_satang_per_gb_month >= 0),
  ai_markup_percent INTEGER NOT NULL CHECK (ai_markup_percent BETWEEN 0 AND 100),
  created_at BIGINT NOT NULL,
  created_by TEXT NOT NULL
);
INSERT INTO billing_target_prices VALUES
  ('holding-usage-2026-10-v1','2026-10',149000,20000000000,500,10,0,'system');

ALTER TABLE billing_invoices ADD COLUMN rating_model TEXT NOT NULL DEFAULT 'legacy'
  CHECK (rating_model IN ('legacy','target'));
INSERT INTO billing_plans VALUES
  ('platform_usage',1,'Platform usage',0,'2026-10',0);

CREATE TABLE billing_target_assignments (
  holding_id TEXT NOT NULL REFERENCES holding_bindings(holding_id),
  effective_month TEXT NOT NULL CHECK (effective_month ~ '^[0-9]{4}-(0[1-9]|1[0-2])$'),
  price_id TEXT NOT NULL REFERENCES billing_target_prices(id),
  mode TEXT NOT NULL CHECK (mode IN ('shadow','sandbox_invoice')),
  service_started_at BIGINT NOT NULL,
  assigned_by TEXT NOT NULL,
  reason TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  PRIMARY KEY (holding_id,effective_month)
);
-- A separate operational designation is required before any target invoice.
-- It is intentionally empty and cannot be populated by the customer UI.
CREATE TABLE billing_target_sandbox_holdings (
  holding_id TEXT PRIMARY KEY REFERENCES holding_bindings(holding_id),
  designated_by TEXT NOT NULL,
  reason TEXT NOT NULL,
  designated_at BIGINT NOT NULL
);

-- Costs are entered only after matching the source meter to provider evidence.
-- Original monetary units and FX ratio remain available for audit; satang is
-- the reviewed amount billed in the Thai Holding ledger.
CREATE TABLE billing_target_provider_costs (
  holding_id TEXT NOT NULL,
  month TEXT NOT NULL,
  source_key TEXT NOT NULL CHECK (source_key ~ '^[a-f0-9]{64}$'),
  provider TEXT NOT NULL,
  statement_id TEXT NOT NULL,
  original_currency TEXT NOT NULL CHECK (original_currency ~ '^[A-Z]{3}$'),
  original_minor BIGINT NOT NULL CHECK (original_minor >= 0),
  fx_numerator BIGINT NOT NULL CHECK (fx_numerator > 0),
  fx_denominator BIGINT NOT NULL CHECK (fx_denominator > 0),
  provider_cost_satang BIGINT NOT NULL CHECK (provider_cost_satang >= 0),
  evidence_id TEXT NOT NULL REFERENCES billing_evidence(id),
  verified_by TEXT NOT NULL,
  verified_at BIGINT NOT NULL,
  PRIMARY KEY (holding_id,month,source_key)
);
CREATE TABLE billing_target_statements (
  holding_id TEXT NOT NULL REFERENCES holding_bindings(holding_id),
  month TEXT NOT NULL,
  provider TEXT NOT NULL,
  statement_id TEXT NOT NULL,
  evidence_id TEXT NOT NULL REFERENCES billing_evidence(id),
  provider_total_minor BIGINT NOT NULL CHECK (provider_total_minor >= 0),
  original_currency TEXT NOT NULL CHECK (original_currency ~ '^[A-Z]{3}$'),
  reconciled_total_minor BIGINT NOT NULL CHECK (reconciled_total_minor >= 0),
  verified_by TEXT NOT NULL,
  verified_at BIGINT NOT NULL,
  PRIMARY KEY (holding_id,month,provider,statement_id)
);

CREATE TABLE billing_target_rated_periods (
  holding_id TEXT NOT NULL REFERENCES holding_bindings(holding_id),
  month TEXT NOT NULL,
  price_id TEXT NOT NULL REFERENCES billing_target_prices(id),
  assignment_month TEXT NOT NULL,
  mode TEXT NOT NULL CHECK (mode IN ('shadow','sandbox_invoice')),
  meter_snapshot_id TEXT NOT NULL,
  service_start BIGINT NOT NULL,
  service_end BIGINT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('held','ready','approved')),
  missing_json TEXT NOT NULL,
  lines_json TEXT NOT NULL,
  total_satang BIGINT,
  rated_at BIGINT NOT NULL,
  approved_by TEXT,
  approved_at BIGINT,
  approval_reason TEXT,
  invoice_id TEXT UNIQUE REFERENCES billing_invoices(id),
  PRIMARY KEY (holding_id,month),
  FOREIGN KEY (holding_id,assignment_month)
    REFERENCES billing_target_assignments(holding_id,effective_month),
  CHECK ((state='held' AND total_satang IS NULL) OR
         (state IN ('ready','approved') AND total_satang IS NOT NULL)),
  CHECK (state<>'approved' OR (invoice_id IS NOT NULL AND approved_by IS NOT NULL AND approval_reason IS NOT NULL))
);
CREATE INDEX billing_target_rated_month ON billing_target_rated_periods(month,state);

-- Re-rating before approval is append-only, so late data never conceals the
-- price and meter snapshot that a reviewer saw earlier in the shadow run.
CREATE TABLE billing_target_rating_runs (
  id TEXT PRIMARY KEY,
  holding_id TEXT NOT NULL REFERENCES holding_bindings(holding_id),
  month TEXT NOT NULL,
  price_id TEXT NOT NULL REFERENCES billing_target_prices(id),
  meter_snapshot_id TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('held','ready')),
  missing_json TEXT NOT NULL,
  lines_json TEXT NOT NULL,
  total_satang BIGINT,
  actor TEXT NOT NULL,
  reason TEXT NOT NULL,
  created_at BIGINT NOT NULL
);
CREATE INDEX billing_target_rating_runs_history
  ON billing_target_rating_runs(holding_id,month,created_at);

CREATE FUNCTION billing_target_immutable_row() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'Target billing evidence and versions are append-only';
END;
$$;
CREATE TRIGGER billing_target_price_immutable BEFORE UPDATE OR DELETE ON billing_target_prices
  FOR EACH ROW EXECUTE FUNCTION billing_target_immutable_row();
CREATE TRIGGER billing_target_assignment_immutable BEFORE UPDATE OR DELETE ON billing_target_assignments
  FOR EACH ROW EXECUTE FUNCTION billing_target_immutable_row();
CREATE TRIGGER billing_target_provider_cost_immutable BEFORE UPDATE OR DELETE ON billing_target_provider_costs
  FOR EACH ROW EXECUTE FUNCTION billing_target_immutable_row();
CREATE TRIGGER billing_target_statement_immutable BEFORE UPDATE OR DELETE ON billing_target_statements
  FOR EACH ROW EXECUTE FUNCTION billing_target_immutable_row();
CREATE TRIGGER billing_target_rating_run_immutable BEFORE UPDATE OR DELETE ON billing_target_rating_runs
  FOR EACH ROW EXECUTE FUNCTION billing_target_immutable_row();
CREATE FUNCTION billing_target_approved_period_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.state='approved' THEN
    RAISE EXCEPTION 'Approved target rating is immutable';
  END IF;
  RETURN NEW;
END;
$$;
CREATE TRIGGER billing_target_period_frozen BEFORE UPDATE OR DELETE ON billing_target_rated_periods
  FOR EACH ROW EXECUTE FUNCTION billing_target_approved_period_immutable();

-- Rollout evidence is separate from Sandbox test designation. A future
-- customer migration needs two complete shadow cycles, verified operating
-- costs, a reviewed margin threshold and primary/backup capacity approval.
CREATE TABLE billing_target_operating_costs (
  holding_id TEXT NOT NULL REFERENCES holding_bindings(holding_id),
  month TEXT NOT NULL,
  category TEXT NOT NULL CHECK (category IN ('infrastructure','backup','stripe','support')),
  amount_satang BIGINT NOT NULL CHECK (amount_satang >= 0),
  evidence_id TEXT NOT NULL REFERENCES billing_evidence(id),
  verified_by TEXT NOT NULL,
  verified_at BIGINT NOT NULL,
  PRIMARY KEY (holding_id,month,category)
);
CREATE TABLE billing_target_capacity_reviews (
  holding_id TEXT NOT NULL REFERENCES holding_bindings(holding_id),
  effective_month TEXT NOT NULL,
  projected_original_bytes BIGINT NOT NULL CHECK (projected_original_bytes >= 0),
  primary_available_bytes BIGINT NOT NULL CHECK (primary_available_bytes >= 0),
  backup_available_bytes BIGINT NOT NULL CHECK (backup_available_bytes >= 0),
  reserved_bytes BIGINT NOT NULL CHECK (reserved_bytes >= 0),
  company_limit_bytes BIGINT NOT NULL CHECK (company_limit_bytes >= 0),
  evidence_id TEXT NOT NULL REFERENCES billing_evidence(id),
  approved_by TEXT NOT NULL,
  reason TEXT NOT NULL,
  approved_at BIGINT NOT NULL,
  PRIMARY KEY (holding_id,effective_month)
);
CREATE TABLE billing_target_rollout_approvals (
  holding_id TEXT NOT NULL REFERENCES holding_bindings(holding_id),
  effective_month TEXT NOT NULL,
  shadow_month_1 TEXT NOT NULL,
  shadow_month_2 TEXT NOT NULL,
  minimum_margin_bps INTEGER NOT NULL CHECK (minimum_margin_bps BETWEEN 0 AND 10000),
  approved_by TEXT NOT NULL,
  reason TEXT NOT NULL,
  approved_at BIGINT NOT NULL,
  PRIMARY KEY (holding_id,effective_month)
);
CREATE TRIGGER billing_target_operating_cost_immutable BEFORE UPDATE OR DELETE ON billing_target_operating_costs
  FOR EACH ROW EXECUTE FUNCTION billing_target_immutable_row();
CREATE TRIGGER billing_target_capacity_immutable BEFORE UPDATE OR DELETE ON billing_target_capacity_reviews
  FOR EACH ROW EXECUTE FUNCTION billing_target_immutable_row();
CREATE TRIGGER billing_target_rollout_immutable BEFORE UPDATE OR DELETE ON billing_target_rollout_approvals
  FOR EACH ROW EXECUTE FUNCTION billing_target_immutable_row();

-- Separately approved setup and extra Support are explicit target-period
-- charges. The old monthly Support package is never stacked on this offer.
CREATE TABLE billing_target_extras (
  id TEXT PRIMARY KEY,
  holding_id TEXT NOT NULL REFERENCES holding_bindings(holding_id),
  month TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('setup','support')),
  description TEXT NOT NULL,
  amount_satang BIGINT NOT NULL CHECK (amount_satang >= 0),
  service_start BIGINT NOT NULL,
  service_end BIGINT NOT NULL CHECK (service_end > service_start),
  evidence_id TEXT NOT NULL REFERENCES billing_evidence(id),
  approved_by TEXT NOT NULL,
  approval_reason TEXT NOT NULL,
  approved_at BIGINT NOT NULL
);
CREATE INDEX billing_target_extras_period ON billing_target_extras(holding_id,month);
CREATE TRIGGER billing_target_extra_immutable BEFORE UPDATE OR DELETE ON billing_target_extras
  FOR EACH ROW EXECUTE FUNCTION billing_target_immutable_row();
