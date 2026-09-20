ALTER TABLE hr_pay_groups ADD COLUMN overtime_multiplier_bps BIGINT NOT NULL DEFAULT 15000 CHECK(overtime_multiplier_bps BETWEEN 10000 AND 50000);
ALTER TABLE hr_pay_groups ADD COLUMN proration_method TEXT NOT NULL DEFAULT 'calendar_days' CHECK(proration_method IN ('calendar_days','none'));

CREATE TABLE hr_payroll_review_exports (
  id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES hr_payroll_runs(id),
  run_revision BIGINT NOT NULL,
  file_key TEXT NOT NULL,
  checksum_sha256 TEXT NOT NULL,
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  UNIQUE(run_id,run_revision)
);
