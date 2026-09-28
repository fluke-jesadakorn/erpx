-- Position pay is a recommendation. Only approved compensation revisions feed payroll.
CREATE TABLE position_pay_defaults (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  position_id TEXT NOT NULL,
  version INTEGER NOT NULL CHECK(version > 0),
  pay_group_id TEXT NOT NULL REFERENCES hr_pay_groups(id),
  currency TEXT NOT NULL CHECK(length(currency)=3 AND currency=upper(currency)),
  monthly_amount_minor BIGINT NOT NULL CHECK(monthly_amount_minor >= 0),
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  UNIQUE(company_id,position_id,version),
  FOREIGN KEY(company_id,position_id) REFERENCES organization_positions(company_id,id)
);
CREATE INDEX position_pay_defaults_latest ON position_pay_defaults(company_id,position_id,version DESC);

-- The mapping points to an immutable policy version. Updating a default adds a row.
CREATE TABLE level_leave_defaults (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  level_id TEXT NOT NULL REFERENCES staff_levels(id),
  version INTEGER NOT NULL CHECK(version > 0),
  policy_version_id TEXT NOT NULL,
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  UNIQUE(company_id,level_id,version),
  FOREIGN KEY(company_id,policy_version_id) REFERENCES leave_policy_versions(company_id,id)
);
CREATE INDEX level_leave_defaults_latest ON level_leave_defaults(company_id,level_id,version DESC);
