PRAGMA foreign_keys = ON;

/* A person is stable across transfers, rehires, and optional login accounts. */
CREATE TABLE IF NOT EXISTS hr_people (
  id TEXT PRIMARY KEY,
  user_id TEXT UNIQUE,
  company_id TEXT NOT NULL,
  legal_name TEXT NOT NULL,
  preferred_name TEXT,
  email TEXT,
  phone TEXT,
  country_code TEXT NOT NULL CHECK(length(country_code)=2 AND country_code=upper(country_code)),
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','inactive','archived')),
  version INTEGER NOT NULL DEFAULT 1 CHECK(version > 0),
  created_by TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;
CREATE INDEX IF NOT EXISTS hr_people_company_idx ON hr_people(company_id,status,legal_name);

CREATE TABLE IF NOT EXISTS hr_engagements (
  id TEXT PRIMARY KEY,
  person_id TEXT NOT NULL REFERENCES hr_people(id),
  company_id TEXT NOT NULL,
  branch_id TEXT,
  department_id TEXT,
  worker_type TEXT NOT NULL CHECK(worker_type IN ('employee','contractor')),
  employment_type TEXT NOT NULL CHECK(employment_type IN ('salaried','hourly','shift','contractor')),
  status TEXT NOT NULL DEFAULT 'onboarding' CHECK(status IN ('draft','onboarding','active','suspended','ended')),
  start_date TEXT NOT NULL,
  end_date TEXT,
  manager_person_id TEXT REFERENCES hr_people(id),
  job_title TEXT NOT NULL,
  currency TEXT NOT NULL CHECK(length(currency)=3 AND currency=upper(currency)),
  version INTEGER NOT NULL DEFAULT 1 CHECK(version > 0),
  created_by TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  CHECK(end_date IS NULL OR end_date >= start_date)
) STRICT;
CREATE INDEX IF NOT EXISTS hr_engagements_scope_idx ON hr_engagements(company_id,branch_id,department_id,status);
CREATE INDEX IF NOT EXISTS hr_engagements_person_idx ON hr_engagements(person_id,start_date);

CREATE TABLE IF NOT EXISTS hr_compensation_revisions (
  id TEXT PRIMARY KEY,
  engagement_id TEXT NOT NULL REFERENCES hr_engagements(id),
  effective_from TEXT NOT NULL,
  effective_to TEXT,
  pay_frequency TEXT NOT NULL CHECK(pay_frequency IN ('weekly','fortnightly','semimonthly','monthly','off_cycle')),
  base_amount_minor INTEGER NOT NULL DEFAULT 0 CHECK(base_amount_minor >= 0),
  hourly_rate_minor INTEGER NOT NULL DEFAULT 0 CHECK(hourly_rate_minor >= 0),
  expected_hours REAL NOT NULL DEFAULT 0 CHECK(expected_hours >= 0),
  currency TEXT NOT NULL CHECK(length(currency)=3 AND currency=upper(currency)),
  components_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(components_json)),
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('draft','approved','superseded')),
  version INTEGER NOT NULL DEFAULT 1 CHECK(version > 0),
  approved_by TEXT,
  created_by TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  CHECK(effective_to IS NULL OR effective_to >= effective_from)
) STRICT;
CREATE INDEX IF NOT EXISTS hr_compensation_effective_idx ON hr_compensation_revisions(engagement_id,effective_from,status);

CREATE TABLE IF NOT EXISTS hr_pay_groups (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  name TEXT NOT NULL,
  country_code TEXT NOT NULL CHECK(length(country_code)=2 AND country_code=upper(country_code)),
  currency TEXT NOT NULL CHECK(length(currency)=3 AND currency=upper(currency)),
  frequency TEXT NOT NULL CHECK(frequency IN ('weekly','fortnightly','semimonthly','monthly')),
  timezone TEXT NOT NULL DEFAULT 'UTC',
  calendar_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(calendar_json)),
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('draft','active','archived')),
  created_by TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE(company_id,name)
) STRICT;

CREATE TABLE IF NOT EXISTS hr_time_entries (
  id TEXT PRIMARY KEY,
  engagement_id TEXT NOT NULL REFERENCES hr_engagements(id),
  company_id TEXT NOT NULL,
  work_date TEXT NOT NULL,
  regular_hours REAL NOT NULL DEFAULT 0 CHECK(regular_hours >= 0),
  overtime_hours REAL NOT NULL DEFAULT 0 CHECK(overtime_hours >= 0),
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','submitted','approved','rejected')),
  note TEXT NOT NULL DEFAULT '',
  version INTEGER NOT NULL DEFAULT 1,
  submitted_by TEXT,
  approved_by TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE(engagement_id,work_date)
) STRICT;
CREATE INDEX IF NOT EXISTS hr_time_entries_period_idx ON hr_time_entries(company_id,work_date,status);

CREATE TABLE IF NOT EXISTS hr_leave_balances (
  id TEXT PRIMARY KEY,
  engagement_id TEXT NOT NULL REFERENCES hr_engagements(id),
  leave_code TEXT NOT NULL,
  year INTEGER NOT NULL,
  opening_days REAL NOT NULL DEFAULT 0,
  accrued_days REAL NOT NULL DEFAULT 0,
  used_days REAL NOT NULL DEFAULT 0,
  carried_days REAL NOT NULL DEFAULT 0,
  version INTEGER NOT NULL DEFAULT 1,
  UNIQUE(engagement_id,leave_code,year)
) STRICT;

CREATE TABLE IF NOT EXISTS hr_country_packs (
  id TEXT PRIMARY KEY,
  country_code TEXT NOT NULL CHECK(length(country_code)=2 AND country_code=upper(country_code)),
  version TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','testing','validated','active','superseded')),
  effective_from TEXT,
  rules_json TEXT NOT NULL CHECK(json_valid(rules_json)),
  export_formats_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(export_formats_json)),
  approval_evidence TEXT,
  created_by TEXT NOT NULL,
  validated_by TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE(country_code,version)
) STRICT;

CREATE TABLE IF NOT EXISTS hr_payroll_runs (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  pay_group_id TEXT NOT NULL REFERENCES hr_pay_groups(id),
  country_pack_id TEXT REFERENCES hr_country_packs(id),
  period_start TEXT NOT NULL,
  period_end TEXT NOT NULL,
  pay_date TEXT NOT NULL,
  currency TEXT NOT NULL CHECK(length(currency)=3 AND currency=upper(currency)),
  mode TEXT NOT NULL DEFAULT 'preview' CHECK(mode IN ('preview','production')),
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','calculating','preview','approved','posted','cancelled')),
  revision INTEGER NOT NULL DEFAULT 1 CHECK(revision > 0),
  totals_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(totals_json)),
  input_digest TEXT,
  error_message TEXT,
  version INTEGER NOT NULL DEFAULT 1 CHECK(version > 0),
  created_by TEXT NOT NULL,
  approved_by TEXT,
  posted_by TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE(company_id,pay_group_id,period_start,period_end,revision),
  CHECK(period_end >= period_start)
) STRICT;
CREATE INDEX IF NOT EXISTS hr_payroll_runs_company_idx ON hr_payroll_runs(company_id,status,pay_date);

CREATE TABLE IF NOT EXISTS hr_payroll_items (
  id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES hr_payroll_runs(id),
  engagement_id TEXT NOT NULL REFERENCES hr_engagements(id),
  worker_type TEXT NOT NULL CHECK(worker_type IN ('employee','contractor')),
  input_snapshot_json TEXT NOT NULL CHECK(json_valid(input_snapshot_json)),
  result_json TEXT NOT NULL CHECK(json_valid(result_json)),
  gross_minor INTEGER NOT NULL DEFAULT 0,
  deductions_minor INTEGER NOT NULL DEFAULT 0,
  employer_cost_minor INTEGER NOT NULL DEFAULT 0,
  net_minor INTEGER NOT NULL DEFAULT 0,
  currency TEXT NOT NULL CHECK(length(currency)=3 AND currency=upper(currency)),
  status TEXT NOT NULL DEFAULT 'calculated' CHECK(status IN ('calculated','excluded','approved','posted')),
  created_at INTEGER NOT NULL,
  UNIQUE(run_id,engagement_id)
) STRICT;
CREATE INDEX IF NOT EXISTS hr_payroll_items_run_idx ON hr_payroll_items(run_id,status);

CREATE TABLE IF NOT EXISTS hr_payment_batches (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  source_type TEXT NOT NULL CHECK(source_type IN ('payroll','expense','contractor')),
  source_id TEXT NOT NULL,
  currency TEXT NOT NULL CHECK(length(currency)=3 AND currency=upper(currency)),
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','approved','exported','partially_settled','settled','cancelled')),
  format_code TEXT NOT NULL DEFAULT 'generic-csv',
  file_key TEXT,
  file_checksum TEXT,
  total_minor INTEGER NOT NULL DEFAULT 0,
  version INTEGER NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,
  approved_by TEXT,
  exported_at INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;
CREATE INDEX IF NOT EXISTS hr_payment_batches_source_idx ON hr_payment_batches(company_id,source_type,source_id,status);

CREATE TABLE IF NOT EXISTS hr_payment_batch_items (
  id TEXT PRIMARY KEY,
  batch_id TEXT NOT NULL REFERENCES hr_payment_batches(id),
  engagement_id TEXT REFERENCES hr_engagements(id),
  person_id TEXT REFERENCES hr_people(id),
  reference TEXT NOT NULL,
  payee_name TEXT NOT NULL,
  bank_account_hint TEXT,
  amount_minor INTEGER NOT NULL CHECK(amount_minor > 0),
  currency TEXT NOT NULL CHECK(length(currency)=3 AND currency=upper(currency)),
  status TEXT NOT NULL DEFAULT 'outstanding' CHECK(status IN ('outstanding','confirmed','failed','settled')),
  confirmation_ref TEXT,
  settled_at INTEGER,
  UNIQUE(batch_id,reference)
) STRICT;

CREATE TABLE IF NOT EXISTS hr_payroll_postings (
  id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES hr_payroll_runs(id),
  journal_id TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  UNIQUE(run_id,journal_id)
) STRICT;
