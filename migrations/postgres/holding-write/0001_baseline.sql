-- PostgreSQL 18 baseline; fresh installations only. No demo data.
-- Source: final schema of holding-write through 0042_work_card_preferences.sql


-- Shared PostgreSQL functions used by the application-owned SQL contract.
CREATE FUNCTION erpx_json_valid(document text) RETURNS boolean LANGUAGE sql IMMUTABLE AS $$ SELECT document IS JSON $$;
CREATE FUNCTION erpx_json_extract(document text, path text) RETURNS text LANGUAGE sql IMMUTABLE AS $$
 SELECT jsonb_path_query_first(document::jsonb,path::jsonpath) #>> '{}' $$;
CREATE FUNCTION erpx_json_each(document text, path text DEFAULT '$') RETURNS TABLE(key text,value text) LANGUAGE sql IMMUTABLE AS $$
 SELECT (ordinality-1)::text, element #>> '{}' FROM jsonb_array_elements(COALESCE(jsonb_path_query_first(document::jsonb,path::jsonpath),'[]'::jsonb)) WITH ORDINALITY AS a(element,ordinality) $$;
CREATE FUNCTION erpx_json(document text) RETURNS jsonb LANGUAGE sql IMMUTABLE AS $$ SELECT document::jsonb $$;
CREATE FUNCTION erpx_json(document jsonb) RETURNS jsonb LANGUAGE sql IMMUTABLE AS $$ SELECT document $$;
CREATE FUNCTION erpx_json_array_append(state jsonb, element anyelement) RETURNS jsonb LANGUAGE sql IMMUTABLE AS $$ SELECT state || jsonb_build_array(element) $$;
CREATE AGGREGATE erpx_json_array_agg(anyelement) (SFUNC=erpx_json_array_append,STYPE=jsonb,INITCOND='[]');


CREATE TABLE assignments (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL,
  role TEXT NOT NULL,
  modules_json TEXT NOT NULL CHECK(erpx_json_valid(modules_json)),
  company_id TEXT NOT NULL,
  branch_id TEXT,
  valid_from BIGINT NOT NULL,
  valid_until BIGINT NOT NULL,
  revoked_at BIGINT,
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0), scope_kind TEXT NOT NULL DEFAULT 'holding'
  CHECK(scope_kind IN ('holding','branch','department')), scope_node_id TEXT,
  CHECK(valid_until > valid_from)
);

CREATE TABLE employees (
  id TEXT PRIMARY KEY,
  user_id TEXT UNIQUE,
  company_id TEXT NOT NULL,
  branch_id TEXT NOT NULL,
  display_name TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','inactive')),
  created_at BIGINT NOT NULL
, department_id TEXT, person_id TEXT );

CREATE TABLE leave_requests (
  id TEXT PRIMARY KEY,
  number TEXT NOT NULL UNIQUE,
  employee_id TEXT NOT NULL ,
  company_id TEXT NOT NULL,
  branch_id TEXT NOT NULL,
  start_date TEXT NOT NULL,
  days BIGINT NOT NULL CHECK(days BETWEEN 1 AND 365),
  handover_note TEXT NOT NULL DEFAULT '',
  state TEXT NOT NULL DEFAULT 'draft' CHECK(state IN ('draft','pending_approval','approved','rejected','cancelled')),
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  created_by TEXT NOT NULL,
  prepared_by TEXT,
  decided_by TEXT,
  decision_note TEXT,
  last_command_id TEXT,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL
, department_id TEXT, person_id TEXT , engagement_id TEXT );

CREATE TABLE command_receipts (
  command_id TEXT PRIMARY KEY,
  operation TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  request_hash TEXT NOT NULL,
  record_id TEXT NOT NULL,
  result_json TEXT,
  created_at BIGINT NOT NULL
);

CREATE TABLE command_assertions (
  command_id TEXT NOT NULL,
  step TEXT NOT NULL,
  ok BIGINT NOT NULL CHECK(ok = 1),
  PRIMARY KEY(command_id, step)
);

CREATE TABLE decision_evidence (
  id TEXT PRIMARY KEY,
  command_id TEXT NOT NULL UNIQUE ,
  record_id TEXT NOT NULL,
  action TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  assignment_id TEXT NOT NULL,
  reviewed_version BIGINT NOT NULL,
  input_digest TEXT NOT NULL,
  created_at BIGINT NOT NULL
);

CREATE TABLE audit_events (
  id TEXT PRIMARY KEY,
  command_id TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  assignment_id TEXT NOT NULL,
  action TEXT NOT NULL,
  record_type TEXT NOT NULL,
  record_id TEXT NOT NULL,
  record_version BIGINT NOT NULL,
  purpose TEXT NOT NULL,
  created_at BIGINT NOT NULL
);

CREATE TABLE outbox_events (
  id TEXT PRIMARY KEY,
  command_id TEXT NOT NULL,
  event_type TEXT NOT NULL,
  aggregate_type TEXT NOT NULL,
  aggregate_id TEXT NOT NULL,
  aggregate_version BIGINT NOT NULL,
  payload_json TEXT NOT NULL CHECK(erpx_json_valid(payload_json)),
  created_at BIGINT NOT NULL,
  published_at BIGINT,
  claim_until BIGINT,
  attempts BIGINT NOT NULL DEFAULT 0
);

CREATE TABLE upload_intents (
  id TEXT PRIMARY KEY,
  owner_type TEXT NOT NULL,
  owner_id TEXT NOT NULL,
  purpose TEXT NOT NULL,
  original_name TEXT NOT NULL,
  media_type TEXT NOT NULL,
  declared_bytes BIGINT NOT NULL CHECK(declared_bytes BETWEEN 1 AND 10485760),
  actual_bytes BIGINT,
  checksum_sha256 TEXT,
  staging_key TEXT NOT NULL UNIQUE,
  final_key TEXT UNIQUE,
  revision BIGINT NOT NULL DEFAULT 1,
  status TEXT NOT NULL CHECK(status IN ('pending','uploaded','ready','failed','expired')),
  created_by TEXT NOT NULL,
  expires_at BIGINT NOT NULL,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL
);

CREATE TABLE attachments (
  id TEXT PRIMARY KEY,
  upload_id TEXT NOT NULL UNIQUE ,
  owner_type TEXT NOT NULL,
  owner_id TEXT NOT NULL,
  purpose TEXT NOT NULL,
  object_key TEXT NOT NULL UNIQUE,
  revision BIGINT NOT NULL,
  original_name TEXT NOT NULL,
  media_type TEXT NOT NULL,
  bytes BIGINT NOT NULL,
  checksum_sha256 TEXT NOT NULL,
  created_by TEXT NOT NULL,
  legal_hold BIGINT NOT NULL DEFAULT 0 CHECK(legal_hold IN (0,1)),
  created_at BIGINT NOT NULL
, revoked_at BIGINT);

CREATE TABLE social_posts (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL DEFAULT 'team' CHECK(kind IN ('team','record','system')),
  author_id TEXT NOT NULL,
  author_name TEXT NOT NULL,
  company_id TEXT NOT NULL,
  branch_id TEXT,
  body TEXT NOT NULL CHECK(length(body) BETWEEN 1 AND 3000),
  linked_module TEXT,
  linked_record_id TEXT,
  linked_record_label TEXT,
  state TEXT NOT NULL DEFAULT 'published' CHECK(state IN ('published','archived','moderated')),
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  last_command_id TEXT,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL, department_id TEXT,
  CHECK((linked_module IS NULL AND linked_record_id IS NULL AND linked_record_label IS NULL) OR
        (linked_module IS NOT NULL AND linked_record_id IS NOT NULL AND linked_record_label IS NOT NULL))
);

CREATE TABLE social_topics (
  tag TEXT PRIMARY KEY CHECK(length(tag) BETWEEN 1 AND 40),
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL
);

CREATE TABLE social_post_topics (
  post_id TEXT NOT NULL ,
  tag TEXT NOT NULL ,
  PRIMARY KEY(post_id, tag)
);

CREATE TABLE social_comments (
  id TEXT PRIMARY KEY,
  post_id TEXT NOT NULL ,
  author_id TEXT NOT NULL,
  author_name TEXT NOT NULL,
  body TEXT NOT NULL CHECK(length(body) BETWEEN 1 AND 1000),
  state TEXT NOT NULL DEFAULT 'published' CHECK(state IN ('published','tombstoned')),
  last_command_id TEXT,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL
, version BIGINT NOT NULL DEFAULT 1);

CREATE TABLE social_reactions (
  post_id TEXT NOT NULL ,
  principal_id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK(kind = 'appreciate'),
  created_at BIGINT NOT NULL,
  PRIMARY KEY(post_id, principal_id, kind)
);

CREATE TABLE social_follows (
  principal_id TEXT NOT NULL,
  target_type TEXT NOT NULL CHECK(target_type IN ('post','person','topic','record')),
  target_id TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  PRIMARY KEY(principal_id, target_type, target_id)
);

CREATE TABLE social_stories (
  id TEXT PRIMARY KEY,
  author_id TEXT NOT NULL,
  author_name TEXT NOT NULL,
  company_id TEXT NOT NULL,
  branch_id TEXT,
  title TEXT NOT NULL CHECK(length(title) BETWEEN 1 AND 80),
  body TEXT NOT NULL CHECK(length(body) BETWEEN 1 AND 500),
  theme TEXT NOT NULL DEFAULT 'sage' CHECK(theme IN ('sage','sunset','sky','plum')),
  state TEXT NOT NULL DEFAULT 'published' CHECK(state IN ('published','archived','moderated')),
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  last_command_id TEXT,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL
, department_id TEXT);

CREATE TABLE organization_nodes (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL CHECK(kind IN ('company','headquarters','branch','department','team')),
  slug TEXT NOT NULL,
  display_name TEXT NOT NULL,
  parent_id TEXT ,
  company_id TEXT NOT NULL,
  branch_id TEXT,
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','suspended')),
  hierarchy_revision BIGINT NOT NULL DEFAULT 1 CHECK(hierarchy_revision > 0),
  UNIQUE(parent_id, slug)
);

CREATE TABLE workspace_records (
  module_id TEXT NOT NULL,
  id TEXT NOT NULL,
  section_id TEXT NOT NULL,
  title TEXT NOT NULL CHECK(length(title) BETWEEN 1 AND 160),
  description TEXT NOT NULL DEFAULT '' CHECK(length(description) <= 2000),
  status TEXT NOT NULL DEFAULT 'draft'
    CHECK(status IN ('draft','in_review','approved','completed','rejected')),
  fields_json TEXT NOT NULL CHECK(erpx_json_valid(fields_json)),
  company_id TEXT NOT NULL,
  branch_id TEXT,
  department_id TEXT,
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  created_by TEXT NOT NULL,
  updated_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL, archived_at BIGINT,
  PRIMARY KEY(module_id, id)
);

CREATE TABLE workspace_record_transitions (
  id TEXT PRIMARY KEY,
  module_id TEXT NOT NULL,
  record_id TEXT NOT NULL,
  from_status TEXT NOT NULL,
  to_status TEXT NOT NULL,
  action TEXT NOT NULL,
  note TEXT NOT NULL DEFAULT '',
  actor_id TEXT NOT NULL,
  assignment_id TEXT NOT NULL,
  record_version BIGINT NOT NULL,
  created_at BIGINT NOT NULL
);

CREATE TABLE record_reference_edges (
  host_module TEXT NOT NULL,
  host_id TEXT NOT NULL,
  target_module TEXT NOT NULL,
  target_id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('dependsOn','blocks','billedBy','fulfills','authoredWith')),
  ordinal BIGINT NOT NULL CHECK(ordinal BETWEEN 0 AND 49),
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  PRIMARY KEY(host_module, host_id, target_module, target_id)
);

CREATE TABLE record_field_suggestions (
  id TEXT PRIMARY KEY,
  module_id TEXT NOT NULL,
  record_id TEXT NOT NULL,
  field_key TEXT NOT NULL,
  proposed_value_json TEXT NOT NULL CHECK(erpx_json_valid(proposed_value_json)),
  previous_value_digest TEXT NOT NULL,
  source_versions_json TEXT NOT NULL CHECK(erpx_json_valid(source_versions_json)),
  provenance_json TEXT NOT NULL CHECK(erpx_json_valid(provenance_json)),
  state TEXT NOT NULL CHECK(state IN ('ready','applied','rejected','conflicted')),
  created_by TEXT NOT NULL,
  applied_by TEXT,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL
);

CREATE TABLE record_principal_mentions (
  host_module TEXT NOT NULL,
  host_id TEXT NOT NULL,
  target_kind TEXT NOT NULL CHECK(target_kind IN ('person','group','agent_pool')),
  target_id TEXT NOT NULL,
  target_label TEXT NOT NULL,
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  PRIMARY KEY(host_module, host_id, target_kind, target_id)
);

CREATE TABLE social_mentions (
  owner_type TEXT NOT NULL CHECK(owner_type IN ('post','comment')),
  owner_id TEXT NOT NULL,
  target_kind TEXT NOT NULL CHECK(target_kind IN ('person','record')),
  target_id TEXT NOT NULL,
  target_label TEXT NOT NULL,
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  PRIMARY KEY(owner_type, owner_id, target_kind, target_id)
);

CREATE TABLE finance_charts (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  name TEXT NOT NULL,
  base_currency TEXT NOT NULL CHECK(length(base_currency)=3 AND base_currency=upper(base_currency)),
  accounting_basis TEXT NOT NULL DEFAULT 'IFRS',
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('draft','active','archived')),
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  UNIQUE(company_id, name)
);

CREATE TABLE finance_accounts (
  id TEXT PRIMARY KEY,
  chart_id TEXT NOT NULL ,
  company_id TEXT NOT NULL,
  code TEXT NOT NULL,
  name TEXT NOT NULL,
  category TEXT NOT NULL CHECK(category IN ('asset','liability','equity','revenue','expense','other_income','other_expense')),
  parent_id TEXT,
  currency TEXT,
  is_control BIGINT NOT NULL DEFAULT 0 CHECK(is_control IN (0,1)),
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','inactive')),
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  UNIQUE(chart_id, code)
);

CREATE TABLE finance_periods (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  name TEXT NOT NULL,
  start_date TEXT NOT NULL,
  end_date TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'open' CHECK(state IN ('open','closing','closed')),
  close_version BIGINT NOT NULL DEFAULT 0,
  closed_by TEXT,
  closed_at BIGINT,
  created_at BIGINT NOT NULL, close_started_by TEXT,
  UNIQUE(company_id, start_date, end_date),
  CHECK(end_date >= start_date)
);

CREATE TABLE finance_journals (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  chart_id TEXT NOT NULL ,
  period_id TEXT NOT NULL ,
  journal_number TEXT NOT NULL,
  entry_date TEXT NOT NULL,
  currency TEXT NOT NULL CHECK(length(currency)=3 AND currency=upper(currency)),
  fx_rate_micros BIGINT,
  source_type TEXT NOT NULL DEFAULT 'manual',
  source_id TEXT,
  memo TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','in_review','posted','reversed')),
  is_opening BIGINT NOT NULL DEFAULT 0 CHECK(is_opening IN (0,1)),
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  created_by TEXT NOT NULL,
  posted_by TEXT,
  posted_at BIGINT,
  reversed_by TEXT,
  reversed_at BIGINT,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  UNIQUE(company_id, journal_number)
);

CREATE TABLE finance_journal_lines (
  id TEXT PRIMARY KEY,
  journal_id TEXT NOT NULL ,
  account_id TEXT NOT NULL ,
  description TEXT NOT NULL DEFAULT '',
  debit_minor BIGINT NOT NULL DEFAULT 0 CHECK(debit_minor >= 0),
  credit_minor BIGINT NOT NULL DEFAULT 0 CHECK(credit_minor >= 0),
  base_debit_minor BIGINT NOT NULL DEFAULT 0 CHECK(base_debit_minor >= 0),
  base_credit_minor BIGINT NOT NULL DEFAULT 0 CHECK(base_credit_minor >= 0),
  dimension_json TEXT NOT NULL DEFAULT '{}' CHECK(erpx_json_valid(dimension_json)),
  ordinal BIGINT NOT NULL CHECK(ordinal >= 0),
  CHECK((debit_minor = 0 AND credit_minor > 0) OR (credit_minor = 0 AND debit_minor > 0))
);

CREATE TABLE finance_parties (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('customer','supplier','employee')),
  code TEXT NOT NULL,
  name TEXT NOT NULL,
  currency TEXT NOT NULL,
  tax_registration TEXT,
  payment_terms_days BIGINT NOT NULL DEFAULT 30,
  credit_limit_minor BIGINT NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','blocked')),
  version BIGINT NOT NULL DEFAULT 1,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL, person_id TEXT ,
  UNIQUE(company_id, kind, code)
);

CREATE TABLE finance_invoices (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  party_id TEXT NOT NULL ,
  kind TEXT NOT NULL CHECK(kind IN ('payable','receivable')),
  invoice_number TEXT NOT NULL,
  invoice_date TEXT NOT NULL,
  due_date TEXT NOT NULL,
  currency TEXT NOT NULL,
  subtotal_minor BIGINT NOT NULL CHECK(subtotal_minor >= 0),
  tax_minor BIGINT NOT NULL DEFAULT 0 CHECK(tax_minor >= 0),
  total_minor BIGINT NOT NULL CHECK(total_minor >= 0),
  outstanding_minor BIGINT NOT NULL CHECK(outstanding_minor >= 0),
  tax_code TEXT,
  debit_account_id TEXT NOT NULL ,
  credit_account_id TEXT NOT NULL ,
  source_type TEXT,
  source_id TEXT,
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','in_review','posted','partially_paid','paid','voided')),
  journal_id TEXT ,
  version BIGINT NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,
  approved_by TEXT,
  posted_at BIGINT,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL, tax_account_id TEXT ,
  UNIQUE(company_id, kind, invoice_number)
);

CREATE TABLE finance_payments (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  party_id TEXT ,
  kind TEXT NOT NULL CHECK(kind IN ('payment','receipt','refund')),
  payment_number TEXT NOT NULL,
  payment_date TEXT NOT NULL,
  currency TEXT NOT NULL,
  amount_minor BIGINT NOT NULL CHECK(amount_minor > 0),
  outstanding_minor BIGINT NOT NULL CHECK(outstanding_minor >= 0),
  bank_account TEXT,
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','in_review','approved','released','settled','voided')),
  journal_id TEXT ,
  version BIGINT NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,
  approved_by TEXT,
  released_by TEXT,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  UNIQUE(company_id, payment_number)
);

CREATE TABLE finance_allocations (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  payment_id TEXT NOT NULL ,
  invoice_id TEXT NOT NULL ,
  amount_minor BIGINT NOT NULL CHECK(amount_minor > 0),
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  UNIQUE(payment_id, invoice_id)
);

CREATE TABLE finance_bank_statements (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  bank_account TEXT NOT NULL,
  statement_number TEXT NOT NULL,
  statement_date TEXT NOT NULL,
  opening_minor BIGINT NOT NULL DEFAULT 0,
  closing_minor BIGINT NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'imported' CHECK(status IN ('imported','matching','reconciled')),
  source_hash TEXT NOT NULL,
  version BIGINT NOT NULL DEFAULT 1,
  imported_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  UNIQUE(company_id, bank_account, source_hash)
);

CREATE TABLE finance_bank_transactions (
  id TEXT PRIMARY KEY,
  statement_id TEXT NOT NULL ,
  company_id TEXT NOT NULL,
  transaction_date TEXT NOT NULL,
  external_id TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  amount_minor BIGINT NOT NULL,
  currency TEXT NOT NULL,
  match_status TEXT NOT NULL DEFAULT 'unmatched' CHECK(match_status IN ('unmatched','proposed','matched','ignored')),
  matched_type TEXT,
  matched_id TEXT,
  version BIGINT NOT NULL DEFAULT 1,
  UNIQUE(statement_id, external_id)
);

CREATE TABLE finance_tax_codes (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  code TEXT NOT NULL,
  name TEXT NOT NULL,
  rate_micros BIGINT NOT NULL CHECK(rate_micros >= 0),
  recoverable_micros BIGINT NOT NULL DEFAULT 1000000 CHECK(recoverable_micros BETWEEN 0 AND 1000000),
  effective_from TEXT NOT NULL,
  effective_to TEXT,
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','inactive')),
  UNIQUE(company_id, code, effective_from)
);

CREATE TABLE finance_assets (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  asset_number TEXT NOT NULL,
  name TEXT NOT NULL,
  account_id TEXT NOT NULL ,
  cost_minor BIGINT NOT NULL CHECK(cost_minor >= 0),
  residual_minor BIGINT NOT NULL DEFAULT 0 CHECK(residual_minor >= 0),
  useful_life_months BIGINT NOT NULL CHECK(useful_life_months > 0),
  in_service_date TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','in_service','impaired','retired')),
  accumulated_depreciation_minor BIGINT NOT NULL DEFAULT 0,
  version BIGINT NOT NULL DEFAULT 1, depreciation_account_id TEXT , currency TEXT NOT NULL DEFAULT 'USD',
  UNIQUE(company_id, asset_number)
);

CREATE TABLE finance_budgets (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  name TEXT NOT NULL,
  fiscal_year BIGINT NOT NULL,
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','submitted','approved','closed')),
  currency TEXT NOT NULL,
  version BIGINT NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  UNIQUE(company_id, name, fiscal_year)
);

CREATE TABLE finance_budget_lines (
  id TEXT PRIMARY KEY,
  budget_id TEXT NOT NULL ,
  account_id TEXT NOT NULL ,
  period TEXT NOT NULL,
  amount_minor BIGINT NOT NULL,
  dimension_json TEXT NOT NULL DEFAULT '{}' CHECK(erpx_json_valid(dimension_json)),
  UNIQUE(budget_id, account_id, period)
);

CREATE TABLE finance_commands (
  command_id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  operation TEXT NOT NULL,
  request_hash TEXT NOT NULL,
  result_json TEXT,
  created_at BIGINT NOT NULL
);

CREATE TABLE finance_accrual_schedules (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  description TEXT NOT NULL,
  debit_account_id TEXT NOT NULL ,
  credit_account_id TEXT NOT NULL ,
  currency TEXT NOT NULL,
  total_minor BIGINT NOT NULL CHECK(total_minor > 0),
  start_date TEXT NOT NULL,
  end_date TEXT NOT NULL,
  next_run_date TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('draft','active','completed','cancelled')),
  version BIGINT NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL
);

CREATE TABLE finance_revenue_contracts (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  customer_id TEXT NOT NULL ,
  contract_number TEXT NOT NULL,
  currency TEXT NOT NULL,
  transaction_price_minor BIGINT NOT NULL CHECK(transaction_price_minor >= 0),
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','approved','active','completed','cancelled')),
  version BIGINT NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  UNIQUE(company_id, contract_number)
);

CREATE TABLE finance_revenue_obligations (
  id TEXT PRIMARY KEY,
  contract_id TEXT NOT NULL ,
  description TEXT NOT NULL,
  allocated_minor BIGINT NOT NULL CHECK(allocated_minor >= 0),
  recognition_method TEXT NOT NULL CHECK(recognition_method IN ('point_in_time','over_time','milestone')),
  start_date TEXT NOT NULL,
  end_date TEXT,
  recognized_minor BIGINT NOT NULL DEFAULT 0,
  revenue_account_id TEXT NOT NULL ,
  deferred_account_id TEXT NOT NULL ,
  status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','active','complete'))
);

CREATE TABLE finance_leases (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  lease_number TEXT NOT NULL,
  counterparty TEXT NOT NULL,
  currency TEXT NOT NULL,
  commencement_date TEXT NOT NULL,
  end_date TEXT NOT NULL,
  payment_frequency TEXT NOT NULL DEFAULT 'monthly',
  payment_minor BIGINT NOT NULL CHECK(payment_minor > 0),
  discount_rate_micros BIGINT NOT NULL CHECK(discount_rate_micros >= 0),
  right_of_use_minor BIGINT NOT NULL DEFAULT 0,
  liability_minor BIGINT NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','active','modified','terminated','completed')),
  version BIGINT NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  UNIQUE(company_id, lease_number)
);

CREATE TABLE finance_loans (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  loan_number TEXT NOT NULL,
  lender TEXT NOT NULL,
  currency TEXT NOT NULL,
  principal_minor BIGINT NOT NULL CHECK(principal_minor > 0),
  annual_rate_micros BIGINT NOT NULL CHECK(annual_rate_micros >= 0),
  commencement_date TEXT NOT NULL,
  maturity_date TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','active','repaid','defaulted')),
  outstanding_minor BIGINT NOT NULL,
  version BIGINT NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  UNIQUE(company_id, loan_number)
);

CREATE TABLE finance_intercompany_transactions (
  id TEXT PRIMARY KEY,
  holding_id TEXT NOT NULL,
  source_company_id TEXT NOT NULL,
  target_company_id TEXT NOT NULL,
  reference TEXT NOT NULL,
  currency TEXT NOT NULL,
  amount_minor BIGINT NOT NULL CHECK(amount_minor > 0),
  source_journal_id TEXT ,
  target_journal_id TEXT ,
  status TEXT NOT NULL DEFAULT 'unmatched' CHECK(status IN ('unmatched','matched','settled','eliminated')),
  version BIGINT NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  UNIQUE(holding_id, reference)
);

CREATE TABLE finance_consolidation_runs (
  id TEXT PRIMARY KEY,
  holding_id TEXT NOT NULL,
  presentation_currency TEXT NOT NULL,
  as_of_date TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','review','approved','published')),
  source_snapshot_json TEXT NOT NULL CHECK(erpx_json_valid(source_snapshot_json)),
  version BIGINT NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,
  approved_by TEXT,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL
);

CREATE TABLE finance_consolidation_eliminations (
  id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL ,
  description TEXT NOT NULL,
  debit_account_id TEXT NOT NULL ,
  credit_account_id TEXT NOT NULL ,
  amount_minor BIGINT NOT NULL CHECK(amount_minor > 0),
  journal_id TEXT ,
  status TEXT NOT NULL DEFAULT 'proposed' CHECK(status IN ('proposed','approved','posted'))
);

CREATE TABLE finance_tax_returns (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  tax_type TEXT NOT NULL,
  period_start TEXT NOT NULL,
  period_end TEXT NOT NULL,
  output_tax_minor BIGINT NOT NULL DEFAULT 0,
  input_tax_minor BIGINT NOT NULL DEFAULT 0,
  withholding_tax_minor BIGINT NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','review','approved','settled','filed')),
  version BIGINT NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL
);

CREATE TABLE hr_people (
  id TEXT PRIMARY KEY,
  user_id TEXT UNIQUE,
  company_id TEXT NOT NULL,
  legal_name TEXT NOT NULL,
  preferred_name TEXT,
  email TEXT,
  phone TEXT,
  country_code TEXT NOT NULL CHECK(length(country_code)=2 AND country_code=upper(country_code)),
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','inactive','archived')),
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL
, merged_into_id TEXT );

CREATE TABLE hr_engagements (
  id TEXT PRIMARY KEY,
  person_id TEXT NOT NULL ,
  company_id TEXT NOT NULL,
  branch_id TEXT,
  department_id TEXT,
  worker_type TEXT NOT NULL CHECK(worker_type IN ('employee','contractor')),
  employment_type TEXT NOT NULL CHECK(employment_type IN ('salaried','hourly','shift','contractor')),
  status TEXT NOT NULL DEFAULT 'onboarding' CHECK(status IN ('draft','onboarding','active','suspended','ended')),
  start_date TEXT NOT NULL,
  end_date TEXT,
  manager_person_id TEXT ,
  job_title TEXT NOT NULL,
  currency TEXT NOT NULL CHECK(length(currency)=3 AND currency=upper(currency)),
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL, pay_group_id TEXT , level_id TEXT ,
  CHECK(end_date IS NULL OR end_date >= start_date)
);

CREATE TABLE hr_compensation_revisions (
  id TEXT PRIMARY KEY,
  engagement_id TEXT NOT NULL ,
  effective_from TEXT NOT NULL,
  effective_to TEXT,
  pay_frequency TEXT NOT NULL CHECK(pay_frequency IN ('weekly','fortnightly','semimonthly','monthly','off_cycle')),
  base_amount_minor BIGINT NOT NULL DEFAULT 0 CHECK(base_amount_minor >= 0),
  hourly_rate_minor BIGINT NOT NULL DEFAULT 0 CHECK(hourly_rate_minor >= 0),
  expected_hours DOUBLE PRECISION NOT NULL DEFAULT 0 CHECK(expected_hours >= 0),
  currency TEXT NOT NULL CHECK(length(currency)=3 AND currency=upper(currency)),
  components_json TEXT NOT NULL DEFAULT '[]' CHECK(erpx_json_valid(components_json)),
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('draft','approved','superseded')),
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  approved_by TEXT,
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  CHECK(effective_to IS NULL OR effective_to >= effective_from)
);

CREATE TABLE hr_pay_groups (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  name TEXT NOT NULL,
  country_code TEXT NOT NULL CHECK(length(country_code)=2 AND country_code=upper(country_code)),
  currency TEXT NOT NULL CHECK(length(currency)=3 AND currency=upper(currency)),
  frequency TEXT NOT NULL CHECK(frequency IN ('weekly','fortnightly','semimonthly','monthly')),
  timezone TEXT NOT NULL DEFAULT 'UTC',
  calendar_json TEXT NOT NULL DEFAULT '{}' CHECK(erpx_json_valid(calendar_json)),
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('draft','active','archived')),
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  UNIQUE(company_id,name)
);

CREATE TABLE hr_time_entries (
  id TEXT PRIMARY KEY,
  engagement_id TEXT NOT NULL ,
  company_id TEXT NOT NULL,
  work_date TEXT NOT NULL,
  regular_hours DOUBLE PRECISION NOT NULL DEFAULT 0 CHECK(regular_hours >= 0),
  overtime_hours DOUBLE PRECISION NOT NULL DEFAULT 0 CHECK(overtime_hours >= 0),
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','submitted','approved','rejected')),
  note TEXT NOT NULL DEFAULT '',
  version BIGINT NOT NULL DEFAULT 1,
  submitted_by TEXT,
  approved_by TEXT,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL, rejection_reason TEXT,
  UNIQUE(engagement_id,work_date)
);

CREATE TABLE hr_leave_balances (
  id TEXT PRIMARY KEY,
  engagement_id TEXT NOT NULL ,
  leave_code TEXT NOT NULL,
  year BIGINT NOT NULL,
  opening_days DOUBLE PRECISION NOT NULL DEFAULT 0,
  accrued_days DOUBLE PRECISION NOT NULL DEFAULT 0,
  used_days DOUBLE PRECISION NOT NULL DEFAULT 0,
  carried_days DOUBLE PRECISION NOT NULL DEFAULT 0,
  version BIGINT NOT NULL DEFAULT 1,
  UNIQUE(engagement_id,leave_code,year)
);

CREATE TABLE hr_country_packs (
  id TEXT PRIMARY KEY,
  country_code TEXT NOT NULL CHECK(length(country_code)=2 AND country_code=upper(country_code)),
  version TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','testing','validated','active','superseded')),
  effective_from TEXT,
  rules_json TEXT NOT NULL CHECK(erpx_json_valid(rules_json)),
  export_formats_json TEXT NOT NULL DEFAULT '[]' CHECK(erpx_json_valid(export_formats_json)),
  approval_evidence TEXT,
  created_by TEXT NOT NULL,
  validated_by TEXT,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  UNIQUE(country_code,version)
);

CREATE TABLE hr_payroll_runs (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  pay_group_id TEXT NOT NULL ,
  country_pack_id TEXT ,
  period_start TEXT NOT NULL,
  period_end TEXT NOT NULL,
  pay_date TEXT NOT NULL,
  currency TEXT NOT NULL CHECK(length(currency)=3 AND currency=upper(currency)),
  mode TEXT NOT NULL DEFAULT 'preview' CHECK(mode IN ('preview','production')),
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','calculating','preview','approved','posted','cancelled')),
  revision BIGINT NOT NULL DEFAULT 1 CHECK(revision > 0),
  totals_json TEXT NOT NULL DEFAULT '{}' CHECK(erpx_json_valid(totals_json)),
  input_digest TEXT,
  error_message TEXT,
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  created_by TEXT NOT NULL,
  approved_by TEXT,
  posted_by TEXT,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  UNIQUE(company_id,pay_group_id,period_start,period_end,revision),
  CHECK(period_end >= period_start)
);

CREATE TABLE hr_payroll_items (
  id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL ,
  engagement_id TEXT NOT NULL ,
  worker_type TEXT NOT NULL CHECK(worker_type IN ('employee','contractor')),
  input_snapshot_json TEXT NOT NULL CHECK(erpx_json_valid(input_snapshot_json)),
  result_json TEXT NOT NULL CHECK(erpx_json_valid(result_json)),
  gross_minor BIGINT NOT NULL DEFAULT 0,
  deductions_minor BIGINT NOT NULL DEFAULT 0,
  employer_cost_minor BIGINT NOT NULL DEFAULT 0,
  net_minor BIGINT NOT NULL DEFAULT 0,
  currency TEXT NOT NULL CHECK(length(currency)=3 AND currency=upper(currency)),
  status TEXT NOT NULL DEFAULT 'calculated' CHECK(status IN ('calculated','excluded','approved','posted')),
  created_at BIGINT NOT NULL,
  UNIQUE(run_id,engagement_id)
);

CREATE TABLE hr_payment_batches (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  source_type TEXT NOT NULL CHECK(source_type IN ('payroll','expense','contractor')),
  source_id TEXT NOT NULL,
  currency TEXT NOT NULL CHECK(length(currency)=3 AND currency=upper(currency)),
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','approved','exported','partially_settled','settled','cancelled')),
  format_code TEXT NOT NULL DEFAULT 'generic-csv',
  file_key TEXT,
  file_checksum TEXT,
  total_minor BIGINT NOT NULL DEFAULT 0,
  version BIGINT NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,
  approved_by TEXT,
  exported_at BIGINT,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL
);

CREATE TABLE hr_payment_batch_items (
  id TEXT PRIMARY KEY,
  batch_id TEXT NOT NULL ,
  engagement_id TEXT ,
  person_id TEXT ,
  reference TEXT NOT NULL,
  payee_name TEXT NOT NULL,
  bank_account_hint TEXT,
  amount_minor BIGINT NOT NULL CHECK(amount_minor > 0),
  currency TEXT NOT NULL CHECK(length(currency)=3 AND currency=upper(currency)),
  status TEXT NOT NULL DEFAULT 'outstanding' CHECK(status IN ('outstanding','confirmed','failed','settled')),
  confirmation_ref TEXT,
  settled_at BIGINT,
  UNIQUE(batch_id,reference)
);

CREATE TABLE hr_payroll_postings (
  id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL ,
  journal_id TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  UNIQUE(run_id,journal_id)
);

CREATE TABLE hr_command_receipts (
  command_id TEXT PRIMARY KEY,
  operation TEXT NOT NULL,
  request_hash TEXT NOT NULL,
  result_json TEXT,
  created_at BIGINT NOT NULL
);

CREATE TABLE seed_repairs (
  repair_id TEXT PRIMARY KEY,
  snapshot_json TEXT NOT NULL CHECK(erpx_json_valid(snapshot_json)),
  created_at BIGINT NOT NULL
);

CREATE TABLE commerce_settings (
  company_id TEXT PRIMARY KEY,
  chart_id TEXT NOT NULL ,
  currency TEXT NOT NULL,
  receivable_account_id TEXT NOT NULL ,
  payable_account_id TEXT NOT NULL ,
  revenue_account_id TEXT NOT NULL ,
  inventory_account_id TEXT NOT NULL ,
  clearing_account_id TEXT NOT NULL ,
  cogs_account_id TEXT NOT NULL ,
  input_tax_account_id TEXT NOT NULL ,
  output_tax_account_id TEXT NOT NULL ,
  cash_account_id TEXT NOT NULL 
);

CREATE TABLE commerce_products (
  id TEXT PRIMARY KEY,company_id TEXT NOT NULL,sku TEXT NOT NULL,name TEXT NOT NULL,
  unit TEXT NOT NULL DEFAULT 'each',price_minor BIGINT NOT NULL CHECK(price_minor>=0),
  tax_bps BIGINT NOT NULL DEFAULT 0 CHECK(tax_bps BETWEEN 0 AND 10000),
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','archived')),
  version BIGINT NOT NULL DEFAULT 1,UNIQUE(company_id,sku)
);

CREATE TABLE commerce_warehouses (
  id TEXT PRIMARY KEY,company_id TEXT NOT NULL,branch_id TEXT NOT NULL ,
  code TEXT NOT NULL,name TEXT NOT NULL,status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','archived')),
  version BIGINT NOT NULL DEFAULT 1,UNIQUE(company_id,code)
);

CREATE TABLE commerce_orders (
  id TEXT PRIMARY KEY,company_id TEXT NOT NULL,branch_id TEXT NOT NULL ,
  department_id TEXT,kind TEXT NOT NULL CHECK(kind IN ('quote','sales','purchase')),
  number TEXT NOT NULL,party_id TEXT NOT NULL ,
  warehouse_id TEXT NOT NULL ,currency TEXT NOT NULL,
  order_date TEXT NOT NULL,due_date TEXT NOT NULL,note TEXT NOT NULL DEFAULT '',
  state TEXT NOT NULL DEFAULT 'draft' CHECK(state IN ('draft','submitted','confirmed','partially_fulfilled','fulfilled','converted','cancelled')),
  subtotal_minor BIGINT NOT NULL CHECK(subtotal_minor>=0),tax_minor BIGINT NOT NULL CHECK(tax_minor>=0),
  source_id TEXT ,version BIGINT NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,approved_by TEXT,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
  UNIQUE(company_id,number),UNIQUE(source_id)
);

CREATE TABLE commerce_order_lines (
  id TEXT PRIMARY KEY,order_id TEXT NOT NULL ,product_id TEXT NOT NULL ,
  description TEXT NOT NULL,quantity_milli BIGINT NOT NULL CHECK(quantity_milli>0),
  fulfilled_milli BIGINT NOT NULL DEFAULT 0 CHECK(fulfilled_milli>=0 AND fulfilled_milli<=quantity_milli),
  price_minor BIGINT NOT NULL CHECK(price_minor>=0),tax_bps BIGINT NOT NULL CHECK(tax_bps BETWEEN 0 AND 10000),
  subtotal_minor BIGINT NOT NULL CHECK(subtotal_minor>=0),tax_minor BIGINT NOT NULL CHECK(tax_minor>=0),
  ordinal BIGINT NOT NULL,UNIQUE(order_id,product_id)
);

CREATE TABLE commerce_stock (
  warehouse_id TEXT NOT NULL ,product_id TEXT NOT NULL ,
  quantity_milli BIGINT NOT NULL DEFAULT 0 CHECK(quantity_milli>=0),value_minor BIGINT NOT NULL DEFAULT 0 CHECK(value_minor>=0),
  version BIGINT NOT NULL DEFAULT 1,PRIMARY KEY(warehouse_id,product_id)
);

CREATE TABLE commerce_fulfillments (
  id TEXT PRIMARY KEY,order_id TEXT NOT NULL ,number TEXT NOT NULL,
  fulfillment_date TEXT NOT NULL,created_by TEXT NOT NULL,created_at BIGINT NOT NULL,
  journal_id TEXT ,invoice_id TEXT UNIQUE ,version BIGINT NOT NULL DEFAULT 1
);

CREATE TABLE commerce_fulfillment_lines (
  id TEXT PRIMARY KEY,fulfillment_id TEXT NOT NULL ,order_line_id TEXT NOT NULL ,
  quantity_milli BIGINT NOT NULL CHECK(quantity_milli>0),value_minor BIGINT NOT NULL CHECK(value_minor>=0),
  subtotal_minor BIGINT NOT NULL CHECK(subtotal_minor>=0),tax_minor BIGINT NOT NULL CHECK(tax_minor>=0),UNIQUE(fulfillment_id,order_line_id)
);

CREATE TABLE commerce_stock_moves (
  id TEXT PRIMARY KEY,warehouse_id TEXT NOT NULL ,product_id TEXT NOT NULL ,
  fulfillment_id TEXT NOT NULL ,quantity_milli BIGINT NOT NULL,value_minor BIGINT NOT NULL,
  created_at BIGINT NOT NULL
);

CREATE TABLE inventory_counts (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  branch_id TEXT NOT NULL ,
  warehouse_id TEXT NOT NULL ,
  number TEXT NOT NULL,
  count_date TEXT NOT NULL,
  reason TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','submitted','rejected','posted','cancelled')),
  version BIGINT NOT NULL DEFAULT 1,
  net_value_minor BIGINT NOT NULL DEFAULT 0,
  created_by TEXT NOT NULL,
  prepared_by TEXT NOT NULL,
  approved_by TEXT,
  review_reason TEXT,
  submitted_at BIGINT,
  posted_at BIGINT,
  journal_id TEXT ,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  UNIQUE(company_id,number)
);

CREATE TABLE inventory_count_lines (
  id TEXT PRIMARY KEY,
  count_id TEXT NOT NULL ,
  product_id TEXT NOT NULL ,
  product_version BIGINT NOT NULL,
  description TEXT NOT NULL,
  unit TEXT NOT NULL,
  snapshot_quantity_milli BIGINT NOT NULL CHECK(snapshot_quantity_milli>=0),
  snapshot_value_minor BIGINT NOT NULL CHECK(snapshot_value_minor>=0),
  snapshot_version BIGINT NOT NULL,
  counted_quantity_milli BIGINT NOT NULL CHECK(counted_quantity_milli>=0),
  counted_value_minor BIGINT NOT NULL CHECK(counted_value_minor>=0),
  unit_cost_minor BIGINT,
  ordinal BIGINT NOT NULL,
  UNIQUE(count_id,product_id)
);

CREATE TABLE inventory_adjustment_moves (
  id TEXT PRIMARY KEY,
  count_id TEXT NOT NULL ,
  warehouse_id TEXT NOT NULL ,
  product_id TEXT NOT NULL ,
  quantity_milli BIGINT NOT NULL,
  value_minor BIGINT NOT NULL,
  movement_date TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  UNIQUE(count_id,product_id)
);

CREATE TABLE inventory_count_decisions (
  id TEXT PRIMARY KEY,
  count_id TEXT NOT NULL ,
  command_id TEXT NOT NULL UNIQUE ,
  action TEXT NOT NULL CHECK(action IN ('post','reject','cancel')),
  actor_id TEXT NOT NULL,
  reviewed_version BIGINT NOT NULL,
  reason TEXT NOT NULL,
  created_at BIGINT NOT NULL
);

CREATE TABLE inventory_transfers (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL,
 source_warehouse_id TEXT NOT NULL ,
 target_warehouse_id TEXT NOT NULL ,
 source_branch_id TEXT NOT NULL ,
 target_branch_id TEXT NOT NULL ,
 source_warehouse_version BIGINT NOT NULL,target_warehouse_version BIGINT NOT NULL,
 number TEXT NOT NULL, transfer_date TEXT NOT NULL, reason TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','submitted','rejected','posted','cancelled')),
 version BIGINT NOT NULL DEFAULT 1, total_value_minor BIGINT NOT NULL CHECK(total_value_minor>=0),
 created_by TEXT NOT NULL,prepared_by TEXT NOT NULL,approved_by TEXT,
 submitted_at BIGINT,posted_at BIGINT,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 CHECK(source_warehouse_id!=target_warehouse_id), UNIQUE(company_id,number)
);

CREATE TABLE inventory_transfer_lines (
 id TEXT PRIMARY KEY,transfer_id TEXT NOT NULL ,
 product_id TEXT NOT NULL ,product_version BIGINT NOT NULL,
 description TEXT NOT NULL,unit TEXT NOT NULL,quantity_milli BIGINT NOT NULL CHECK(quantity_milli>0),
 value_minor BIGINT NOT NULL CHECK(value_minor>0),
 source_quantity_milli BIGINT NOT NULL,source_value_minor BIGINT NOT NULL,source_version BIGINT NOT NULL,
 ordinal BIGINT NOT NULL,UNIQUE(transfer_id,product_id)
);

CREATE TABLE inventory_transfer_moves (
 id TEXT PRIMARY KEY,transfer_id TEXT NOT NULL ,
 warehouse_id TEXT NOT NULL ,product_id TEXT NOT NULL ,
 quantity_milli BIGINT NOT NULL,value_minor BIGINT NOT NULL,movement_date TEXT NOT NULL,created_at BIGINT NOT NULL,
 UNIQUE(transfer_id,warehouse_id,product_id)
);

CREATE TABLE inventory_transfer_decisions (
 id TEXT PRIMARY KEY,transfer_id TEXT NOT NULL ,
 command_id TEXT NOT NULL UNIQUE ,
 action TEXT NOT NULL CHECK(action IN ('post','reject','cancel')),actor_id TEXT NOT NULL,
 reviewed_version BIGINT NOT NULL,reason TEXT NOT NULL,created_at BIGINT NOT NULL
);

CREATE TABLE staff_levels (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, code TEXT NOT NULL, name TEXT NOT NULL,
  rank BIGINT NOT NULL, active BIGINT NOT NULL DEFAULT 1 CHECK(active IN (0,1)),
  version BIGINT NOT NULL DEFAULT 1, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
  UNIQUE(company_id,code)
);

CREATE TABLE staff_level_history (
  id TEXT PRIMARY KEY, engagement_id TEXT NOT NULL ,
  previous_level_id TEXT , level_id TEXT NOT NULL ,
  actor_id TEXT NOT NULL, reason TEXT NOT NULL, command_id TEXT NOT NULL, created_at BIGINT NOT NULL
);

CREATE TABLE department_heads (
  department_id TEXT PRIMARY KEY , company_id TEXT NOT NULL,
  person_id TEXT NOT NULL , version BIGINT NOT NULL DEFAULT 1,
  updated_by TEXT NOT NULL, updated_at BIGINT NOT NULL
);

CREATE TABLE staff_projects (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, branch_id TEXT, department_id TEXT,
  title TEXT NOT NULL, description TEXT NOT NULL DEFAULT '', owner_user_id TEXT NOT NULL,
  version BIGINT NOT NULL DEFAULT 1, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
);

CREATE TABLE staff_project_members (
  project_id TEXT NOT NULL , user_id TEXT NOT NULL,
  added_by TEXT NOT NULL, created_at BIGINT NOT NULL, PRIMARY KEY(project_id,user_id)
);

CREATE TABLE routine_templates (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, branch_id TEXT, department_id TEXT,
  title TEXT NOT NULL, description TEXT NOT NULL DEFAULT '', creator_user_id TEXT NOT NULL,
  assignee_user_id TEXT NOT NULL, project_id TEXT ,
  start_approvers_json TEXT NOT NULL DEFAULT '[]' CHECK(erpx_json_valid(start_approvers_json)),
  completion_approvers_json TEXT NOT NULL DEFAULT '[]' CHECK(erpx_json_valid(completion_approvers_json)),
  published BIGINT NOT NULL DEFAULT 0 CHECK(published IN (0,1)),
  version BIGINT NOT NULL DEFAULT 1, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
);

CREATE TABLE routine_schedules (
  id TEXT PRIMARY KEY, template_id TEXT NOT NULL , company_id TEXT NOT NULL,
  frequency TEXT NOT NULL CHECK(frequency IN ('daily','weekly','monthly')),
  timezone TEXT NOT NULL DEFAULT 'Asia/Bangkok', occurrence_time TEXT NOT NULL DEFAULT '09:00',
  start_date TEXT NOT NULL, end_date TEXT, weekday BIGINT CHECK(weekday BETWEEN 0 AND 6),
  month_day BIGINT CHECK(month_day BETWEEN 1 AND 31), due_offset_hours BIGINT NOT NULL DEFAULT 24 CHECK(due_offset_hours BETWEEN 0 AND 8760),
  state TEXT NOT NULL DEFAULT 'paused' CHECK(state IN ('active','paused','ended')),
  next_occurrence_at BIGINT, paused_at BIGINT, last_generated_at BIGINT, last_error TEXT,
  version BIGINT NOT NULL DEFAULT 1, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
);

CREATE TABLE routine_schedule_pauses (
  id TEXT PRIMARY KEY, schedule_id TEXT NOT NULL ,
  started_at BIGINT NOT NULL, ended_at BIGINT
);

CREATE TABLE staff_tasks (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, branch_id TEXT, department_id TEXT,
  title TEXT NOT NULL, description TEXT NOT NULL DEFAULT '', creator_user_id TEXT NOT NULL,
  assignee_user_id TEXT NOT NULL, due_at BIGINT NOT NULL, project_id TEXT ,
  completion_note TEXT NOT NULL DEFAULT '',
  start_approvers_json TEXT NOT NULL DEFAULT '[]' CHECK(erpx_json_valid(start_approvers_json)),
  completion_approvers_json TEXT NOT NULL DEFAULT '[]' CHECK(erpx_json_valid(completion_approvers_json)),
  state TEXT NOT NULL DEFAULT 'draft' CHECK(state IN ('draft','start_approval','ready','in_progress','completion_approval','rework','completed','cancelled')),
  blocked_reason TEXT, revision BIGINT NOT NULL DEFAULT 1, version BIGINT NOT NULL DEFAULT 1,
  template_id TEXT , template_version BIGINT,
  schedule_id TEXT , occurrence_at BIGINT,
  created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
  UNIQUE(schedule_id,occurrence_at)
);

CREATE TABLE approval_instances (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, branch_id TEXT, department_id TEXT,
  record_type TEXT NOT NULL CHECK(record_type IN ('expense','leave','task')),
  record_id TEXT NOT NULL, phase TEXT NOT NULL CHECK(phase IN ('request','start','completion')),
  cycle BIGINT NOT NULL, requester_user_id TEXT NOT NULL, assignee_user_id TEXT,
  state TEXT NOT NULL CHECK(state IN ('pending','blocked','approved','rejected','cancelled','superseded')),
  current_step BIGINT NOT NULL DEFAULT 0, blocked_reason TEXT, version BIGINT NOT NULL DEFAULT 1,
  created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
  UNIQUE(record_type,record_id,phase,cycle)
);

CREATE TABLE approval_steps (
  id TEXT PRIMARY KEY, instance_id TEXT NOT NULL , ordinal BIGINT NOT NULL,
  approver_user_id TEXT NOT NULL, authority TEXT NOT NULL CHECK(authority IN ('named','department','hr_admin','holding_owner')),
  state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','approved','rejected')), approver_pool_json TEXT CHECK(approver_pool_json IS NULL OR erpx_json_valid(approver_pool_json)), required_role TEXT, stage TEXT, route_snapshot_json TEXT CHECK(route_snapshot_json IS NULL OR erpx_json_valid(route_snapshot_json)),
  UNIQUE(instance_id,ordinal)
);

CREATE TABLE approval_decisions (
  id TEXT PRIMARY KEY, instance_id TEXT NOT NULL , step_id TEXT ,
  action TEXT NOT NULL CHECK(action IN ('approve','reject','cancel','revise','reroute','initialize')),
  actor_user_id TEXT NOT NULL, previous_approver_user_id TEXT, approver_user_id TEXT,
  reason TEXT NOT NULL, reviewed_version BIGINT NOT NULL, command_id TEXT NOT NULL,
  created_at BIGINT NOT NULL
, authority_json TEXT
  CHECK(authority_json IS NULL OR erpx_json_valid(authority_json)));

CREATE TABLE staff_notifications (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, user_id TEXT NOT NULL, record_type TEXT NOT NULL,
  record_id TEXT NOT NULL, record_version BIGINT NOT NULL, message TEXT NOT NULL,
  created_at BIGINT NOT NULL, read_at BIGINT,
  UNIQUE(user_id,record_type,record_id,record_version,message)
);

CREATE TABLE beta_policy (
  company_id TEXT PRIMARY KEY, member_limit BIGINT NOT NULL, branch_limit BIGINT NOT NULL,
  ai_monthly_limit BIGINT NOT NULL, storage_bytes BIGINT NOT NULL,
  created_at BIGINT NOT NULL
);

CREATE TABLE social_post_drafts (
  id TEXT PRIMARY KEY, author_id TEXT NOT NULL, company_id TEXT NOT NULL, branch_id TEXT,
  body TEXT NOT NULL DEFAULT '' CHECK(length(body)<=3000), version BIGINT NOT NULL DEFAULT 1,
  published_post_id TEXT UNIQUE , created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
, department_id TEXT);

CREATE TABLE social_task_origins (
  post_id TEXT PRIMARY KEY , task_id TEXT NOT NULL UNIQUE ,
  source_version BIGINT NOT NULL, source_hash TEXT NOT NULL, ai_draft_id TEXT,
  created_by TEXT NOT NULL, created_at BIGINT NOT NULL
);

CREATE TABLE social_ai_drafts (
  id TEXT PRIMARY KEY, post_id TEXT NOT NULL , actor_id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('summary','task')), source_version BIGINT NOT NULL,
  source_hash TEXT NOT NULL, comment_ids_json TEXT NOT NULL CHECK(erpx_json_valid(comment_ids_json)),
  sources_json TEXT NOT NULL CHECK(erpx_json_valid(sources_json)), result_json TEXT CHECK(result_json IS NULL OR erpx_json_valid(result_json)),
  state TEXT NOT NULL CHECK(state IN ('pending','ready','failed')), model TEXT NOT NULL,
  created_at BIGINT NOT NULL, expires_at BIGINT NOT NULL
, provider_json TEXT CHECK(provider_json IS NULL OR erpx_json_valid(provider_json)));

CREATE TABLE social_ai_usage (
  id TEXT PRIMARY KEY , company_id TEXT NOT NULL, actor_id TEXT NOT NULL,
  month TEXT NOT NULL, state TEXT NOT NULL CHECK(state IN ('reserved','succeeded','failed')),
  input_tokens BIGINT NOT NULL DEFAULT 0, output_tokens BIGINT NOT NULL DEFAULT 0,
  lease_until BIGINT NOT NULL, created_at BIGINT NOT NULL
);

CREATE TABLE social_reports (
  id TEXT PRIMARY KEY, post_id TEXT NOT NULL , reporter_id TEXT NOT NULL,
  reason TEXT NOT NULL CHECK(length(reason) BETWEEN 1 AND 2000), created_at BIGINT NOT NULL,
  UNIQUE(post_id,reporter_id)
);

CREATE TABLE beta_feedback (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, actor_id TEXT NOT NULL,
  body TEXT NOT NULL CHECK(length(body) BETWEEN 1 AND 3000), created_at BIGINT NOT NULL
);

CREATE TABLE beta_members (
  user_id TEXT PRIMARY KEY, invitation_id TEXT UNIQUE, joined_at BIGINT NOT NULL
);

CREATE TABLE beta_team_invitations (
 id TEXT PRIMARY KEY, email TEXT NOT NULL, branch_id TEXT NOT NULL ,
 invited_by TEXT NOT NULL, token_hash TEXT NOT NULL UNIQUE, expires_at BIGINT NOT NULL,
 accepted_user_id TEXT, accepted_at BIGINT, revoked_at BIGINT, created_at BIGINT NOT NULL
, role TEXT NOT NULL DEFAULT 'employee' CHECK(role IN ('employee','it_admin')));

CREATE TABLE beta_bootstrap (
  singleton BIGINT PRIMARY KEY CHECK(singleton=1), holding_id TEXT NOT NULL UNIQUE,
  owner_user_id TEXT NOT NULL, created_at BIGINT NOT NULL
);

CREATE TABLE "finance_expenses" (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, employee_id TEXT,
  expense_date TEXT, merchant TEXT, currency TEXT,
  amount_minor BIGINT CHECK(amount_minor IS NULL OR amount_minor > 0),
  tax_minor BIGINT NOT NULL DEFAULT 0 CHECK(tax_minor >= 0),
  category_account_id TEXT ,
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','in_review','approved','rejected','posted','reimbursed')),
  journal_id TEXT ,
  evidence_json TEXT NOT NULL DEFAULT '[]' CHECK(erpx_json_valid(evidence_json)),
  version BIGINT NOT NULL DEFAULT 1, created_by TEXT NOT NULL, approved_by TEXT,
  created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
  branch_id TEXT , department_id TEXT ,
  engagement_id TEXT , business_purpose TEXT NOT NULL DEFAULT '',
  receipt_reference TEXT, rejection_reason TEXT, prepared_by TEXT, submitted_at BIGINT,
  posted_at BIGINT, reimbursed_at BIGINT, cancelled_at BIGINT,
  invoice_id TEXT , reversal_journal_id TEXT ,
  claimant_user_id TEXT, claimant_name TEXT,
  workflow_version BIGINT NOT NULL DEFAULT 1 CHECK(workflow_version IN (1,2)),
  workflow_stage TEXT NOT NULL DEFAULT 'draft' CHECK(workflow_stage IN ('draft','department_approval','accounts_review','accounts_approval','payment','completed','cancelled')),
  accounting_json TEXT NOT NULL DEFAULT '{}' CHECK(erpx_json_valid(accounting_json)),
  CHECK(workflow_version=1 OR status IN ('draft','rejected') OR
    (claimant_user_id IS NOT NULL AND employee_id IS NOT NULL AND engagement_id IS NOT NULL
    AND department_id IS NOT NULL AND expense_date IS NOT NULL AND length(merchant)>=2
    AND currency IS NOT NULL AND amount_minor IS NOT NULL AND category_account_id IS NOT NULL
    AND length(business_purpose)>=5 AND length(receipt_reference)>=2))
);

CREATE TABLE "expense_decisions" (
  id TEXT PRIMARY KEY,
  expense_id TEXT NOT NULL ,
  command_id TEXT NOT NULL UNIQUE ,
  action TEXT NOT NULL CHECK(action IN ('approve','reject','cancel')),
  actor_id TEXT NOT NULL, reviewed_version BIGINT NOT NULL,
  reason TEXT NOT NULL, created_at BIGINT NOT NULL
);

CREATE TABLE expense_payment_drafts (
 id TEXT PRIMARY KEY, expense_id TEXT NOT NULL , company_id TEXT NOT NULL,
 created_by TEXT NOT NULL, payment_date TEXT, amount_minor BIGINT CHECK(amount_minor IS NULL OR amount_minor>0),
 reference TEXT, version BIGINT NOT NULL DEFAULT 1,
 status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','confirmed','cancelled')),
 payment_id TEXT , created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
);

CREATE TABLE ai_connections (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, name TEXT NOT NULL,
 version BIGINT NOT NULL DEFAULT 1, draft_revision BIGINT NOT NULL DEFAULT 1,
 active_revision BIGINT, disabled BIGINT NOT NULL DEFAULT 0 CHECK(disabled IN (0,1)),
 deleted_at BIGINT, created_by TEXT NOT NULL, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
);

CREATE TABLE ai_connection_revisions (
 connection_id TEXT NOT NULL , revision BIGINT NOT NULL,
 base_url TEXT NOT NULL, auth_mode TEXT NOT NULL CHECK(auth_mode IN ('bearer','none')),
 credential_json TEXT CHECK(credential_json IS NULL OR erpx_json_valid(credential_json)),
 text_model TEXT NOT NULL, image_model TEXT,
 tested_at BIGINT, text_verified BIGINT NOT NULL DEFAULT 0, image_verified BIGINT NOT NULL DEFAULT 0,
 test_error TEXT, created_by TEXT NOT NULL, created_at BIGINT NOT NULL,
 PRIMARY KEY(connection_id,revision)
);

CREATE TABLE ai_company_settings (
 company_id TEXT PRIMARY KEY, version BIGINT NOT NULL DEFAULT 1,
 mode TEXT NOT NULL DEFAULT 'deployment' CHECK(mode IN ('deployment','custom','disabled')),
  text_connection_id TEXT , image_connection_id TEXT ,
  image_mode TEXT NOT NULL DEFAULT 'default' CHECK(image_mode IN ('default','custom','disabled')),
 updated_by TEXT NOT NULL, updated_at BIGINT NOT NULL
);

CREATE TABLE ai_provider_usage (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, connection_id TEXT, revision BIGINT,
 model TEXT NOT NULL, feature TEXT NOT NULL, input_tokens BIGINT NOT NULL DEFAULT 0,
 output_tokens BIGINT NOT NULL DEFAULT 0, status TEXT NOT NULL, created_at BIGINT NOT NULL
);

CREATE TABLE expense_ai_proposals (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, expense_id TEXT NOT NULL ,
 payment_draft_id TEXT , stage TEXT NOT NULL,
 source_version BIGINT NOT NULL, evidence_digest TEXT NOT NULL, provider_json TEXT NOT NULL CHECK(erpx_json_valid(provider_json)),
 state TEXT NOT NULL CHECK(state IN ('queued','running','ready','failed','stale','applied')),
 result_json TEXT CHECK(result_json IS NULL OR erpx_json_valid(result_json)), error TEXT,
 requested_by TEXT NOT NULL, claim_until BIGINT, attempts BIGINT NOT NULL DEFAULT 0,
 created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
, lease_token TEXT);

CREATE TABLE attachment_derivatives (
 id TEXT PRIMARY KEY, attachment_id TEXT NOT NULL , company_id TEXT NOT NULL,
 page_number BIGINT NOT NULL CHECK(page_number>0), object_key TEXT NOT NULL UNIQUE,
 checksum_sha256 TEXT NOT NULL, original_checksum TEXT NOT NULL, bytes BIGINT NOT NULL CHECK(bytes>0),
 created_by TEXT NOT NULL, created_at BIGINT NOT NULL,
 UNIQUE(attachment_id,page_number,original_checksum)
);

CREATE TABLE expense_ai_usage (
 id TEXT PRIMARY KEY , company_id TEXT NOT NULL, actor_id TEXT NOT NULL,
 month TEXT NOT NULL, state TEXT NOT NULL CHECK(state IN ('reserved','succeeded','failed')),
 lease_until BIGINT NOT NULL, input_tokens BIGINT NOT NULL DEFAULT 0,
 output_tokens BIGINT NOT NULL DEFAULT 0, created_at BIGINT NOT NULL
);

CREATE TABLE attachment_documents (
 attachment_id TEXT PRIMARY KEY , company_id TEXT NOT NULL,
 original_checksum TEXT NOT NULL, page_count BIGINT NOT NULL CHECK(page_count BETWEEN 1 AND 20),
 created_at BIGINT NOT NULL
);

CREATE TABLE service_teams (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, name TEXT NOT NULL,
 active BIGINT NOT NULL DEFAULT 1 CHECK(active IN (0,1)), version BIGINT NOT NULL DEFAULT 1, configuration_json TEXT NOT NULL DEFAULT '{}',
 created_by TEXT NOT NULL, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
 UNIQUE(company_id,id)
);

CREATE TABLE service_team_members (
 company_id TEXT NOT NULL, team_id TEXT NOT NULL, user_id TEXT NOT NULL,
 responsibility TEXT NOT NULL CHECK(responsibility IN ('lead','member')),
 PRIMARY KEY(team_id,user_id)
);

CREATE TABLE service_team_coverage (
 company_id TEXT NOT NULL, team_id TEXT NOT NULL, category TEXT NOT NULL,
 scope_kind TEXT NOT NULL CHECK(scope_kind IN ('holding','branch','department')), scope_node_id TEXT NOT NULL,
 PRIMARY KEY(company_id,category,scope_kind,scope_node_id)
);

CREATE TABLE employee_requests (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, branch_id TEXT, department_id TEXT,
 kind TEXT NOT NULL CHECK(kind IN ('service','equipment','purchase')),
 category TEXT NOT NULL, title TEXT NOT NULL DEFAULT '', description TEXT NOT NULL DEFAULT '',
 location TEXT NOT NULL DEFAULT '', impact TEXT NOT NULL DEFAULT 'normal', needed_date TEXT,
 items_json TEXT NOT NULL DEFAULT '[]', currency TEXT NOT NULL DEFAULT '',
 requester_user_id TEXT NOT NULL, created_by TEXT NOT NULL, parent_request_id TEXT,
 state TEXT NOT NULL DEFAULT 'draft' CHECK(state IN ('draft','returned','blocked','awaiting_approval','queued','in_progress','waiting_requester','resolved','fulfilled','closed','rejected','cancelled')),
 team_id TEXT, assignee_user_id TEXT, routing_json TEXT NOT NULL DEFAULT '{}',
 approval_json TEXT NOT NULL DEFAULT '{}', blocked_reason TEXT,
 resolution TEXT NOT NULL DEFAULT '', purchase_order_id TEXT, handover_json TEXT NOT NULL DEFAULT '{}',
 version BIGINT NOT NULL DEFAULT 1, evidence_version BIGINT NOT NULL DEFAULT 1,
 created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
 UNIQUE(company_id,id)
);

CREATE TABLE employee_request_events (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, request_id TEXT NOT NULL, request_version BIGINT NOT NULL,
 action TEXT NOT NULL, actor_id TEXT NOT NULL, note TEXT NOT NULL DEFAULT '', evidence_json TEXT NOT NULL DEFAULT '{}',
 created_at BIGINT NOT NULL
);

CREATE TABLE handbook_editors (
 company_id TEXT NOT NULL, user_id TEXT NOT NULL, granted_by TEXT NOT NULL, granted_at BIGINT NOT NULL,
 PRIMARY KEY(company_id,user_id)
);

CREATE TABLE handbook_settings (company_id TEXT PRIMARY KEY, version BIGINT NOT NULL);

CREATE TABLE handbook_articles (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, version BIGINT NOT NULL DEFAULT 1,
 draft_revision BIGINT NOT NULL DEFAULT 1, published_revision BIGINT,
 archived BIGINT NOT NULL DEFAULT 0 CHECK(archived IN (0,1)), created_by TEXT NOT NULL,
 created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL, UNIQUE(company_id,id)
);

CREATE TABLE handbook_revisions (
 company_id TEXT NOT NULL, article_id TEXT NOT NULL, revision BIGINT NOT NULL,
 title TEXT NOT NULL, category TEXT NOT NULL, body TEXT NOT NULL,
 created_by TEXT NOT NULL, created_at BIGINT NOT NULL, published_by TEXT, published_at BIGINT,
 PRIMARY KEY(article_id,revision)
);

CREATE TABLE request_ai_proposals (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, request_id TEXT NOT NULL, source_version BIGINT NOT NULL,
 provider_json TEXT NOT NULL, actor_json TEXT NOT NULL, state TEXT NOT NULL CHECK(state IN ('queued','running','ready','failed','stale')),
 result_json TEXT, error TEXT, attempts BIGINT NOT NULL DEFAULT 0, lease_token TEXT, claim_until BIGINT,
 created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
 UNIQUE(company_id,request_id,source_version,provider_json)
);

CREATE TABLE request_ai_usage (
 id TEXT PRIMARY KEY ,company_id TEXT NOT NULL,actor_id TEXT NOT NULL,
 month TEXT NOT NULL,state TEXT NOT NULL CHECK(state IN ('reserved','succeeded','failed')),
 lease_until BIGINT NOT NULL,input_tokens BIGINT NOT NULL DEFAULT 0,output_tokens BIGINT NOT NULL DEFAULT 0,created_at BIGINT NOT NULL
);

CREATE TABLE service_team_revisions (
  company_id TEXT NOT NULL,
  team_id TEXT NOT NULL,
  version BIGINT NOT NULL,
  configuration_json TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  PRIMARY KEY(team_id,version)
);

CREATE TABLE work_card_preferences (
  company_id TEXT NOT NULL ,
  user_id TEXT NOT NULL,
  pinned_ids_json TEXT CHECK(pinned_ids_json IS NULL OR (erpx_json_valid(pinned_ids_json) AND jsonb_typeof(pinned_ids_json::jsonb)='array' AND jsonb_array_length(pinned_ids_json::jsonb)<=6)),
  version BIGINT NOT NULL CHECK(version > 0),
  updated_at BIGINT NOT NULL,
  PRIMARY KEY(company_id,user_id)
);

ALTER TABLE "employees" ADD CONSTRAINT "employees_fk_0" FOREIGN KEY ("person_id") REFERENCES "hr_people" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "leave_requests" ADD CONSTRAINT "leave_requests_fk_0" FOREIGN KEY ("engagement_id") REFERENCES "hr_engagements" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "leave_requests" ADD CONSTRAINT "leave_requests_fk_1" FOREIGN KEY ("person_id") REFERENCES "hr_people" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "leave_requests" ADD CONSTRAINT "leave_requests_fk_2" FOREIGN KEY ("employee_id") REFERENCES "employees" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "decision_evidence" ADD CONSTRAINT "decision_evidence_fk_0" FOREIGN KEY ("command_id") REFERENCES "command_receipts" ("command_id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "attachments" ADD CONSTRAINT "attachments_fk_0" FOREIGN KEY ("upload_id") REFERENCES "upload_intents" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "social_post_topics" ADD CONSTRAINT "social_post_topics_fk_0" FOREIGN KEY ("tag") REFERENCES "social_topics" ("tag") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "social_post_topics" ADD CONSTRAINT "social_post_topics_fk_1" FOREIGN KEY ("post_id") REFERENCES "social_posts" ("id") ON DELETE CASCADE ON UPDATE NO ACTION;

ALTER TABLE "social_comments" ADD CONSTRAINT "social_comments_fk_0" FOREIGN KEY ("post_id") REFERENCES "social_posts" ("id") ON DELETE CASCADE ON UPDATE NO ACTION;

ALTER TABLE "social_reactions" ADD CONSTRAINT "social_reactions_fk_0" FOREIGN KEY ("post_id") REFERENCES "social_posts" ("id") ON DELETE CASCADE ON UPDATE NO ACTION;

ALTER TABLE "organization_nodes" ADD CONSTRAINT "organization_nodes_fk_0" FOREIGN KEY ("parent_id") REFERENCES "organization_nodes" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "workspace_record_transitions" ADD CONSTRAINT "workspace_record_transitions_fk_0" FOREIGN KEY ("module_id","record_id") REFERENCES "workspace_records" ("module_id","id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "record_reference_edges" ADD CONSTRAINT "record_reference_edges_fk_0" FOREIGN KEY ("target_module","target_id") REFERENCES "workspace_records" ("module_id","id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "record_reference_edges" ADD CONSTRAINT "record_reference_edges_fk_1" FOREIGN KEY ("host_module","host_id") REFERENCES "workspace_records" ("module_id","id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "record_field_suggestions" ADD CONSTRAINT "record_field_suggestions_fk_0" FOREIGN KEY ("module_id","record_id") REFERENCES "workspace_records" ("module_id","id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "record_principal_mentions" ADD CONSTRAINT "record_principal_mentions_fk_0" FOREIGN KEY ("host_module","host_id") REFERENCES "workspace_records" ("module_id","id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_accounts" ADD CONSTRAINT "finance_accounts_fk_0" FOREIGN KEY ("parent_id") REFERENCES "finance_accounts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_accounts" ADD CONSTRAINT "finance_accounts_fk_1" FOREIGN KEY ("chart_id") REFERENCES "finance_charts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_journals" ADD CONSTRAINT "finance_journals_fk_0" FOREIGN KEY ("period_id") REFERENCES "finance_periods" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_journals" ADD CONSTRAINT "finance_journals_fk_1" FOREIGN KEY ("chart_id") REFERENCES "finance_charts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_journal_lines" ADD CONSTRAINT "finance_journal_lines_fk_0" FOREIGN KEY ("account_id") REFERENCES "finance_accounts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_journal_lines" ADD CONSTRAINT "finance_journal_lines_fk_1" FOREIGN KEY ("journal_id") REFERENCES "finance_journals" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_parties" ADD CONSTRAINT "finance_parties_fk_0" FOREIGN KEY ("person_id") REFERENCES "hr_people" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_invoices" ADD CONSTRAINT "finance_invoices_fk_0" FOREIGN KEY ("tax_account_id") REFERENCES "finance_accounts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_invoices" ADD CONSTRAINT "finance_invoices_fk_1" FOREIGN KEY ("journal_id") REFERENCES "finance_journals" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_invoices" ADD CONSTRAINT "finance_invoices_fk_2" FOREIGN KEY ("credit_account_id") REFERENCES "finance_accounts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_invoices" ADD CONSTRAINT "finance_invoices_fk_3" FOREIGN KEY ("debit_account_id") REFERENCES "finance_accounts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_invoices" ADD CONSTRAINT "finance_invoices_fk_4" FOREIGN KEY ("party_id") REFERENCES "finance_parties" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_payments" ADD CONSTRAINT "finance_payments_fk_0" FOREIGN KEY ("journal_id") REFERENCES "finance_journals" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_payments" ADD CONSTRAINT "finance_payments_fk_1" FOREIGN KEY ("party_id") REFERENCES "finance_parties" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_allocations" ADD CONSTRAINT "finance_allocations_fk_0" FOREIGN KEY ("invoice_id") REFERENCES "finance_invoices" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_allocations" ADD CONSTRAINT "finance_allocations_fk_1" FOREIGN KEY ("payment_id") REFERENCES "finance_payments" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_bank_transactions" ADD CONSTRAINT "finance_bank_transactions_fk_0" FOREIGN KEY ("statement_id") REFERENCES "finance_bank_statements" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_assets" ADD CONSTRAINT "finance_assets_fk_0" FOREIGN KEY ("depreciation_account_id") REFERENCES "finance_accounts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_assets" ADD CONSTRAINT "finance_assets_fk_1" FOREIGN KEY ("account_id") REFERENCES "finance_accounts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_budget_lines" ADD CONSTRAINT "finance_budget_lines_fk_0" FOREIGN KEY ("account_id") REFERENCES "finance_accounts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_budget_lines" ADD CONSTRAINT "finance_budget_lines_fk_1" FOREIGN KEY ("budget_id") REFERENCES "finance_budgets" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_accrual_schedules" ADD CONSTRAINT "finance_accrual_schedules_fk_0" FOREIGN KEY ("credit_account_id") REFERENCES "finance_accounts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_accrual_schedules" ADD CONSTRAINT "finance_accrual_schedules_fk_1" FOREIGN KEY ("debit_account_id") REFERENCES "finance_accounts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_revenue_contracts" ADD CONSTRAINT "finance_revenue_contracts_fk_0" FOREIGN KEY ("customer_id") REFERENCES "finance_parties" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_revenue_obligations" ADD CONSTRAINT "finance_revenue_obligations_fk_0" FOREIGN KEY ("deferred_account_id") REFERENCES "finance_accounts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_revenue_obligations" ADD CONSTRAINT "finance_revenue_obligations_fk_1" FOREIGN KEY ("revenue_account_id") REFERENCES "finance_accounts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_revenue_obligations" ADD CONSTRAINT "finance_revenue_obligations_fk_2" FOREIGN KEY ("contract_id") REFERENCES "finance_revenue_contracts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_intercompany_transactions" ADD CONSTRAINT "finance_intercompany_transactions_fk_0" FOREIGN KEY ("target_journal_id") REFERENCES "finance_journals" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_intercompany_transactions" ADD CONSTRAINT "finance_intercompany_transactions_fk_1" FOREIGN KEY ("source_journal_id") REFERENCES "finance_journals" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_consolidation_eliminations" ADD CONSTRAINT "finance_consolidation_eliminations_fk_0" FOREIGN KEY ("journal_id") REFERENCES "finance_journals" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_consolidation_eliminations" ADD CONSTRAINT "finance_consolidation_eliminations_fk_1" FOREIGN KEY ("credit_account_id") REFERENCES "finance_accounts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_consolidation_eliminations" ADD CONSTRAINT "finance_consolidation_eliminations_fk_2" FOREIGN KEY ("debit_account_id") REFERENCES "finance_accounts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_consolidation_eliminations" ADD CONSTRAINT "finance_consolidation_eliminations_fk_3" FOREIGN KEY ("run_id") REFERENCES "finance_consolidation_runs" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "hr_people" ADD CONSTRAINT "hr_people_fk_0" FOREIGN KEY ("merged_into_id") REFERENCES "hr_people" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "hr_engagements" ADD CONSTRAINT "hr_engagements_fk_0" FOREIGN KEY ("level_id") REFERENCES "staff_levels" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "hr_engagements" ADD CONSTRAINT "hr_engagements_fk_1" FOREIGN KEY ("pay_group_id") REFERENCES "hr_pay_groups" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "hr_engagements" ADD CONSTRAINT "hr_engagements_fk_2" FOREIGN KEY ("manager_person_id") REFERENCES "hr_people" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "hr_engagements" ADD CONSTRAINT "hr_engagements_fk_3" FOREIGN KEY ("person_id") REFERENCES "hr_people" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "hr_compensation_revisions" ADD CONSTRAINT "hr_compensation_revisions_fk_0" FOREIGN KEY ("engagement_id") REFERENCES "hr_engagements" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "hr_time_entries" ADD CONSTRAINT "hr_time_entries_fk_0" FOREIGN KEY ("engagement_id") REFERENCES "hr_engagements" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "hr_leave_balances" ADD CONSTRAINT "hr_leave_balances_fk_0" FOREIGN KEY ("engagement_id") REFERENCES "hr_engagements" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "hr_payroll_runs" ADD CONSTRAINT "hr_payroll_runs_fk_0" FOREIGN KEY ("country_pack_id") REFERENCES "hr_country_packs" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "hr_payroll_runs" ADD CONSTRAINT "hr_payroll_runs_fk_1" FOREIGN KEY ("pay_group_id") REFERENCES "hr_pay_groups" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "hr_payroll_items" ADD CONSTRAINT "hr_payroll_items_fk_0" FOREIGN KEY ("engagement_id") REFERENCES "hr_engagements" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "hr_payroll_items" ADD CONSTRAINT "hr_payroll_items_fk_1" FOREIGN KEY ("run_id") REFERENCES "hr_payroll_runs" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "hr_payment_batch_items" ADD CONSTRAINT "hr_payment_batch_items_fk_0" FOREIGN KEY ("person_id") REFERENCES "hr_people" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "hr_payment_batch_items" ADD CONSTRAINT "hr_payment_batch_items_fk_1" FOREIGN KEY ("engagement_id") REFERENCES "hr_engagements" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "hr_payment_batch_items" ADD CONSTRAINT "hr_payment_batch_items_fk_2" FOREIGN KEY ("batch_id") REFERENCES "hr_payment_batches" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "hr_payroll_postings" ADD CONSTRAINT "hr_payroll_postings_fk_0" FOREIGN KEY ("run_id") REFERENCES "hr_payroll_runs" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "commerce_settings" ADD CONSTRAINT "commerce_settings_fk_0" FOREIGN KEY ("cash_account_id") REFERENCES "finance_accounts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "commerce_settings" ADD CONSTRAINT "commerce_settings_fk_1" FOREIGN KEY ("output_tax_account_id") REFERENCES "finance_accounts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "commerce_settings" ADD CONSTRAINT "commerce_settings_fk_2" FOREIGN KEY ("input_tax_account_id") REFERENCES "finance_accounts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "commerce_settings" ADD CONSTRAINT "commerce_settings_fk_3" FOREIGN KEY ("cogs_account_id") REFERENCES "finance_accounts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "commerce_settings" ADD CONSTRAINT "commerce_settings_fk_4" FOREIGN KEY ("clearing_account_id") REFERENCES "finance_accounts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "commerce_settings" ADD CONSTRAINT "commerce_settings_fk_5" FOREIGN KEY ("inventory_account_id") REFERENCES "finance_accounts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "commerce_settings" ADD CONSTRAINT "commerce_settings_fk_6" FOREIGN KEY ("revenue_account_id") REFERENCES "finance_accounts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "commerce_settings" ADD CONSTRAINT "commerce_settings_fk_7" FOREIGN KEY ("payable_account_id") REFERENCES "finance_accounts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "commerce_settings" ADD CONSTRAINT "commerce_settings_fk_8" FOREIGN KEY ("receivable_account_id") REFERENCES "finance_accounts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "commerce_settings" ADD CONSTRAINT "commerce_settings_fk_9" FOREIGN KEY ("chart_id") REFERENCES "finance_charts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "commerce_warehouses" ADD CONSTRAINT "commerce_warehouses_fk_0" FOREIGN KEY ("branch_id") REFERENCES "organization_nodes" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "commerce_orders" ADD CONSTRAINT "commerce_orders_fk_0" FOREIGN KEY ("source_id") REFERENCES "commerce_orders" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "commerce_orders" ADD CONSTRAINT "commerce_orders_fk_1" FOREIGN KEY ("warehouse_id") REFERENCES "commerce_warehouses" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "commerce_orders" ADD CONSTRAINT "commerce_orders_fk_2" FOREIGN KEY ("party_id") REFERENCES "finance_parties" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "commerce_orders" ADD CONSTRAINT "commerce_orders_fk_3" FOREIGN KEY ("branch_id") REFERENCES "organization_nodes" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "commerce_order_lines" ADD CONSTRAINT "commerce_order_lines_fk_0" FOREIGN KEY ("product_id") REFERENCES "commerce_products" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "commerce_order_lines" ADD CONSTRAINT "commerce_order_lines_fk_1" FOREIGN KEY ("order_id") REFERENCES "commerce_orders" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "commerce_stock" ADD CONSTRAINT "commerce_stock_fk_0" FOREIGN KEY ("product_id") REFERENCES "commerce_products" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "commerce_stock" ADD CONSTRAINT "commerce_stock_fk_1" FOREIGN KEY ("warehouse_id") REFERENCES "commerce_warehouses" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "commerce_fulfillments" ADD CONSTRAINT "commerce_fulfillments_fk_0" FOREIGN KEY ("invoice_id") REFERENCES "finance_invoices" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "commerce_fulfillments" ADD CONSTRAINT "commerce_fulfillments_fk_1" FOREIGN KEY ("journal_id") REFERENCES "finance_journals" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "commerce_fulfillments" ADD CONSTRAINT "commerce_fulfillments_fk_2" FOREIGN KEY ("order_id") REFERENCES "commerce_orders" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "commerce_fulfillment_lines" ADD CONSTRAINT "commerce_fulfillment_lines_fk_0" FOREIGN KEY ("order_line_id") REFERENCES "commerce_order_lines" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "commerce_fulfillment_lines" ADD CONSTRAINT "commerce_fulfillment_lines_fk_1" FOREIGN KEY ("fulfillment_id") REFERENCES "commerce_fulfillments" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "commerce_stock_moves" ADD CONSTRAINT "commerce_stock_moves_fk_0" FOREIGN KEY ("fulfillment_id") REFERENCES "commerce_fulfillments" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "commerce_stock_moves" ADD CONSTRAINT "commerce_stock_moves_fk_1" FOREIGN KEY ("product_id") REFERENCES "commerce_products" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "commerce_stock_moves" ADD CONSTRAINT "commerce_stock_moves_fk_2" FOREIGN KEY ("warehouse_id") REFERENCES "commerce_warehouses" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "inventory_counts" ADD CONSTRAINT "inventory_counts_fk_0" FOREIGN KEY ("journal_id") REFERENCES "finance_journals" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "inventory_counts" ADD CONSTRAINT "inventory_counts_fk_1" FOREIGN KEY ("warehouse_id") REFERENCES "commerce_warehouses" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "inventory_counts" ADD CONSTRAINT "inventory_counts_fk_2" FOREIGN KEY ("branch_id") REFERENCES "organization_nodes" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "inventory_count_lines" ADD CONSTRAINT "inventory_count_lines_fk_0" FOREIGN KEY ("product_id") REFERENCES "commerce_products" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "inventory_count_lines" ADD CONSTRAINT "inventory_count_lines_fk_1" FOREIGN KEY ("count_id") REFERENCES "inventory_counts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "inventory_adjustment_moves" ADD CONSTRAINT "inventory_adjustment_moves_fk_0" FOREIGN KEY ("product_id") REFERENCES "commerce_products" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "inventory_adjustment_moves" ADD CONSTRAINT "inventory_adjustment_moves_fk_1" FOREIGN KEY ("warehouse_id") REFERENCES "commerce_warehouses" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "inventory_adjustment_moves" ADD CONSTRAINT "inventory_adjustment_moves_fk_2" FOREIGN KEY ("count_id") REFERENCES "inventory_counts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "inventory_count_decisions" ADD CONSTRAINT "inventory_count_decisions_fk_0" FOREIGN KEY ("command_id") REFERENCES "command_receipts" ("command_id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "inventory_count_decisions" ADD CONSTRAINT "inventory_count_decisions_fk_1" FOREIGN KEY ("count_id") REFERENCES "inventory_counts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "inventory_transfers" ADD CONSTRAINT "inventory_transfers_fk_0" FOREIGN KEY ("target_branch_id") REFERENCES "organization_nodes" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "inventory_transfers" ADD CONSTRAINT "inventory_transfers_fk_1" FOREIGN KEY ("source_branch_id") REFERENCES "organization_nodes" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "inventory_transfers" ADD CONSTRAINT "inventory_transfers_fk_2" FOREIGN KEY ("target_warehouse_id") REFERENCES "commerce_warehouses" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "inventory_transfers" ADD CONSTRAINT "inventory_transfers_fk_3" FOREIGN KEY ("source_warehouse_id") REFERENCES "commerce_warehouses" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "inventory_transfer_lines" ADD CONSTRAINT "inventory_transfer_lines_fk_0" FOREIGN KEY ("product_id") REFERENCES "commerce_products" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "inventory_transfer_lines" ADD CONSTRAINT "inventory_transfer_lines_fk_1" FOREIGN KEY ("transfer_id") REFERENCES "inventory_transfers" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "inventory_transfer_moves" ADD CONSTRAINT "inventory_transfer_moves_fk_0" FOREIGN KEY ("product_id") REFERENCES "commerce_products" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "inventory_transfer_moves" ADD CONSTRAINT "inventory_transfer_moves_fk_1" FOREIGN KEY ("warehouse_id") REFERENCES "commerce_warehouses" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "inventory_transfer_moves" ADD CONSTRAINT "inventory_transfer_moves_fk_2" FOREIGN KEY ("transfer_id") REFERENCES "inventory_transfers" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "inventory_transfer_decisions" ADD CONSTRAINT "inventory_transfer_decisions_fk_0" FOREIGN KEY ("command_id") REFERENCES "command_receipts" ("command_id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "inventory_transfer_decisions" ADD CONSTRAINT "inventory_transfer_decisions_fk_1" FOREIGN KEY ("transfer_id") REFERENCES "inventory_transfers" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "staff_level_history" ADD CONSTRAINT "staff_level_history_fk_0" FOREIGN KEY ("level_id") REFERENCES "staff_levels" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "staff_level_history" ADD CONSTRAINT "staff_level_history_fk_1" FOREIGN KEY ("previous_level_id") REFERENCES "staff_levels" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "staff_level_history" ADD CONSTRAINT "staff_level_history_fk_2" FOREIGN KEY ("engagement_id") REFERENCES "hr_engagements" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "department_heads" ADD CONSTRAINT "department_heads_fk_0" FOREIGN KEY ("person_id") REFERENCES "hr_people" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "department_heads" ADD CONSTRAINT "department_heads_fk_1" FOREIGN KEY ("department_id") REFERENCES "organization_nodes" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "staff_project_members" ADD CONSTRAINT "staff_project_members_fk_0" FOREIGN KEY ("project_id") REFERENCES "staff_projects" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "routine_templates" ADD CONSTRAINT "routine_templates_fk_0" FOREIGN KEY ("project_id") REFERENCES "staff_projects" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "routine_schedules" ADD CONSTRAINT "routine_schedules_fk_0" FOREIGN KEY ("template_id") REFERENCES "routine_templates" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "routine_schedule_pauses" ADD CONSTRAINT "routine_schedule_pauses_fk_0" FOREIGN KEY ("schedule_id") REFERENCES "routine_schedules" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "staff_tasks" ADD CONSTRAINT "staff_tasks_fk_0" FOREIGN KEY ("schedule_id") REFERENCES "routine_schedules" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "staff_tasks" ADD CONSTRAINT "staff_tasks_fk_1" FOREIGN KEY ("template_id") REFERENCES "routine_templates" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "staff_tasks" ADD CONSTRAINT "staff_tasks_fk_2" FOREIGN KEY ("project_id") REFERENCES "staff_projects" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "approval_steps" ADD CONSTRAINT "approval_steps_fk_0" FOREIGN KEY ("instance_id") REFERENCES "approval_instances" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "approval_decisions" ADD CONSTRAINT "approval_decisions_fk_0" FOREIGN KEY ("step_id") REFERENCES "approval_steps" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "approval_decisions" ADD CONSTRAINT "approval_decisions_fk_1" FOREIGN KEY ("instance_id") REFERENCES "approval_instances" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "social_post_drafts" ADD CONSTRAINT "social_post_drafts_fk_0" FOREIGN KEY ("published_post_id") REFERENCES "social_posts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "social_task_origins" ADD CONSTRAINT "social_task_origins_fk_0" FOREIGN KEY ("task_id") REFERENCES "staff_tasks" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "social_task_origins" ADD CONSTRAINT "social_task_origins_fk_1" FOREIGN KEY ("post_id") REFERENCES "social_posts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "social_ai_drafts" ADD CONSTRAINT "social_ai_drafts_fk_0" FOREIGN KEY ("post_id") REFERENCES "social_posts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "social_ai_usage" ADD CONSTRAINT "social_ai_usage_fk_0" FOREIGN KEY ("id") REFERENCES "social_ai_drafts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "social_reports" ADD CONSTRAINT "social_reports_fk_0" FOREIGN KEY ("post_id") REFERENCES "social_posts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "beta_team_invitations" ADD CONSTRAINT "beta_team_invitations_fk_0" FOREIGN KEY ("branch_id") REFERENCES "organization_nodes" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_expenses" ADD CONSTRAINT "finance_expenses_fk_0" FOREIGN KEY ("reversal_journal_id") REFERENCES "finance_journals" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_expenses" ADD CONSTRAINT "finance_expenses_fk_1" FOREIGN KEY ("invoice_id") REFERENCES "finance_invoices" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_expenses" ADD CONSTRAINT "finance_expenses_fk_2" FOREIGN KEY ("engagement_id") REFERENCES "hr_engagements" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_expenses" ADD CONSTRAINT "finance_expenses_fk_3" FOREIGN KEY ("department_id") REFERENCES "organization_nodes" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_expenses" ADD CONSTRAINT "finance_expenses_fk_4" FOREIGN KEY ("branch_id") REFERENCES "organization_nodes" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_expenses" ADD CONSTRAINT "finance_expenses_fk_5" FOREIGN KEY ("journal_id") REFERENCES "finance_journals" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "finance_expenses" ADD CONSTRAINT "finance_expenses_fk_6" FOREIGN KEY ("category_account_id") REFERENCES "finance_accounts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "expense_decisions" ADD CONSTRAINT "expense_decisions_fk_0" FOREIGN KEY ("command_id") REFERENCES "command_receipts" ("command_id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "expense_decisions" ADD CONSTRAINT "expense_decisions_fk_1" FOREIGN KEY ("expense_id") REFERENCES "finance_expenses" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "expense_payment_drafts" ADD CONSTRAINT "expense_payment_drafts_fk_0" FOREIGN KEY ("payment_id") REFERENCES "finance_payments" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "expense_payment_drafts" ADD CONSTRAINT "expense_payment_drafts_fk_1" FOREIGN KEY ("expense_id") REFERENCES "finance_expenses" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "ai_connection_revisions" ADD CONSTRAINT "ai_connection_revisions_fk_0" FOREIGN KEY ("connection_id") REFERENCES "ai_connections" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "ai_company_settings" ADD CONSTRAINT "ai_company_settings_fk_0" FOREIGN KEY ("image_connection_id") REFERENCES "ai_connections" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "ai_company_settings" ADD CONSTRAINT "ai_company_settings_fk_1" FOREIGN KEY ("text_connection_id") REFERENCES "ai_connections" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "expense_ai_proposals" ADD CONSTRAINT "expense_ai_proposals_fk_0" FOREIGN KEY ("payment_draft_id") REFERENCES "expense_payment_drafts" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "expense_ai_proposals" ADD CONSTRAINT "expense_ai_proposals_fk_1" FOREIGN KEY ("expense_id") REFERENCES "finance_expenses" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "attachment_derivatives" ADD CONSTRAINT "attachment_derivatives_fk_0" FOREIGN KEY ("attachment_id") REFERENCES "attachments" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "expense_ai_usage" ADD CONSTRAINT "expense_ai_usage_fk_0" FOREIGN KEY ("id") REFERENCES "expense_ai_proposals" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "attachment_documents" ADD CONSTRAINT "attachment_documents_fk_0" FOREIGN KEY ("attachment_id") REFERENCES "attachments" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "service_team_members" ADD CONSTRAINT "service_team_members_fk_0" FOREIGN KEY ("company_id","team_id") REFERENCES "service_teams" ("company_id","id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "service_team_coverage" ADD CONSTRAINT "service_team_coverage_fk_0" FOREIGN KEY ("company_id","team_id") REFERENCES "service_teams" ("company_id","id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "employee_requests" ADD CONSTRAINT "employee_requests_fk_0" FOREIGN KEY ("company_id","parent_request_id") REFERENCES "employee_requests" ("company_id","id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "employee_requests" ADD CONSTRAINT "employee_requests_fk_1" FOREIGN KEY ("company_id","team_id") REFERENCES "service_teams" ("company_id","id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "employee_request_events" ADD CONSTRAINT "employee_request_events_fk_0" FOREIGN KEY ("company_id","request_id") REFERENCES "employee_requests" ("company_id","id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "handbook_revisions" ADD CONSTRAINT "handbook_revisions_fk_0" FOREIGN KEY ("company_id","article_id") REFERENCES "handbook_articles" ("company_id","id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "request_ai_proposals" ADD CONSTRAINT "request_ai_proposals_fk_0" FOREIGN KEY ("company_id","request_id") REFERENCES "employee_requests" ("company_id","id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "request_ai_usage" ADD CONSTRAINT "request_ai_usage_fk_0" FOREIGN KEY ("id") REFERENCES "request_ai_proposals" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "service_team_revisions" ADD CONSTRAINT "service_team_revisions_fk_0" FOREIGN KEY ("company_id","team_id") REFERENCES "service_teams" ("company_id","id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "work_card_preferences" ADD CONSTRAINT "work_card_preferences_fk_0" FOREIGN KEY ("company_id") REFERENCES "organization_nodes" ("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

CREATE INDEX assignment_active_idx ON assignments(user_id, valid_from, valid_until);

CREATE INDEX leave_queue_idx ON leave_requests(state, company_id, branch_id, updated_at DESC);

CREATE INDEX audit_record_idx ON audit_events(record_type, record_id, created_at);

CREATE INDEX outbox_pending_idx ON outbox_events(published_at, claim_until, created_at);

CREATE INDEX upload_owner_idx ON upload_intents(owner_type, owner_id, status);

CREATE INDEX attachment_owner_idx ON attachments(owner_type, owner_id, created_at);

CREATE INDEX social_feed_idx
  ON social_posts(company_id, branch_id, state, created_at DESC);

CREATE INDEX social_topic_posts_idx ON social_post_topics(tag, post_id);

CREATE INDEX social_comments_post_idx ON social_comments(post_id, created_at);

CREATE INDEX social_stories_scope_idx
  ON social_stories(company_id, branch_id, state, created_at DESC);

CREATE UNIQUE INDEX organization_branch_slug_idx
  ON organization_nodes(slug) WHERE kind = 'branch';

CREATE UNIQUE INDEX organization_holding_department_slug_idx
  ON organization_nodes(slug) WHERE kind = 'department' AND branch_id IS NULL;

CREATE INDEX workspace_records_scope_idx
  ON workspace_records(company_id, branch_id, department_id, module_id, section_id, updated_at DESC);

CREATE INDEX workspace_record_transitions_idx
  ON workspace_record_transitions(module_id, record_id, created_at DESC);

CREATE INDEX record_reference_backlinks_idx
  ON record_reference_edges(target_module, target_id, created_at DESC);

CREATE INDEX record_field_suggestions_idx
  ON record_field_suggestions(module_id, record_id, state, created_at DESC);

CREATE INDEX record_principal_mentions_target_idx
  ON record_principal_mentions(target_kind, target_id, created_at DESC);

CREATE INDEX social_mentions_target_idx
  ON social_mentions(target_kind, target_id, created_at DESC);

CREATE INDEX finance_accounts_company_idx ON finance_accounts(company_id, chart_id, status, code);

CREATE INDEX finance_periods_company_date_idx ON finance_periods(company_id, start_date, end_date);

CREATE INDEX finance_journals_company_date_idx ON finance_journals(company_id, entry_date, status);

CREATE INDEX finance_journal_lines_account_idx ON finance_journal_lines(account_id, journal_id);

CREATE INDEX finance_invoices_aging_idx ON finance_invoices(company_id, kind, status, due_date);

CREATE INDEX finance_commands_company_idx ON finance_commands(company_id, operation, created_at);

CREATE INDEX hr_people_company_idx ON hr_people(company_id,status,legal_name);

CREATE INDEX hr_engagements_scope_idx ON hr_engagements(company_id,branch_id,department_id,status);

CREATE INDEX hr_engagements_person_idx ON hr_engagements(person_id,start_date);

CREATE INDEX hr_compensation_effective_idx ON hr_compensation_revisions(engagement_id,effective_from,status);

CREATE INDEX hr_time_entries_period_idx ON hr_time_entries(company_id,work_date,status);

CREATE INDEX hr_payroll_runs_company_idx ON hr_payroll_runs(company_id,status,pay_date);

CREATE INDEX hr_payroll_items_run_idx ON hr_payroll_items(run_id,status);

CREATE INDEX hr_payment_batches_source_idx ON hr_payment_batches(company_id,source_type,source_id,status);

CREATE INDEX hr_engagements_pay_group_idx ON hr_engagements(pay_group_id,status);

CREATE UNIQUE INDEX employee_person_company_idx ON employees(person_id,company_id) WHERE person_id IS NOT NULL;

CREATE INDEX commerce_order_scope_idx ON commerce_orders(company_id,branch_id,kind,state);

CREATE UNIQUE INDEX expense_payee_idx ON finance_parties(company_id,person_id) WHERE person_id IS NOT NULL;

CREATE INDEX inventory_counts_scope_idx ON inventory_counts(company_id,branch_id,status);

CREATE INDEX inventory_count_decisions_count_idx ON inventory_count_decisions(count_id,created_at);

CREATE INDEX inventory_transfers_scope_idx ON inventory_transfers(company_id,source_branch_id,target_branch_id,status);

CREATE INDEX inventory_transfer_decisions_idx ON inventory_transfer_decisions(transfer_id,created_at);

CREATE INDEX staff_level_history_engagement ON staff_level_history(engagement_id,created_at);

CREATE INDEX routine_schedule_due ON routine_schedules(state,next_occurrence_at);

CREATE INDEX routine_pause_schedule ON routine_schedule_pauses(schedule_id,started_at);

CREATE INDEX staff_tasks_company ON staff_tasks(company_id,state,updated_at);

CREATE INDEX staff_tasks_assignee ON staff_tasks(company_id,assignee_user_id,state);

CREATE UNIQUE INDEX approval_one_open ON approval_instances(record_type,record_id,phase) WHERE state IN ('pending','blocked');

CREATE INDEX approval_record ON approval_instances(company_id,record_type,record_id,created_at);

CREATE INDEX approval_user ON approval_steps(approver_user_id,instance_id);

CREATE INDEX approval_decision_history ON approval_decisions(instance_id,created_at);

CREATE INDEX staff_notification_user ON staff_notifications(company_id,user_id,created_at);

CREATE INDEX social_ai_actor ON social_ai_drafts(actor_id,created_at);

CREATE INDEX social_ai_quota ON social_ai_usage(company_id,month);

CREATE UNIQUE INDEX beta_pending_email ON beta_team_invitations(email) WHERE accepted_at IS NULL AND revoked_at IS NULL;

CREATE INDEX expense_decisions_claim_idx ON expense_decisions(expense_id,created_at);

CREATE UNIQUE INDEX expense_invoice_idx ON finance_expenses(invoice_id) WHERE invoice_id IS NOT NULL;

CREATE UNIQUE INDEX expense_receipt_idx ON finance_expenses(company_id,employee_id,lower(merchant),lower(receipt_reference))
 WHERE receipt_reference IS NOT NULL AND cancelled_at IS NULL;

CREATE INDEX expense_scope_idx ON finance_expenses(company_id,branch_id,department_id,status);

CREATE INDEX expense_claimant_idx ON finance_expenses(company_id,claimant_user_id,workflow_stage);

CREATE INDEX expense_payment_drafts_claim ON expense_payment_drafts(expense_id,status);

CREATE INDEX ai_connection_company ON ai_connections(company_id,deleted_at);

CREATE INDEX expense_ai_pending ON expense_ai_proposals(state,claim_until);

CREATE INDEX expense_ai_claim ON expense_ai_proposals(expense_id,created_at);

CREATE INDEX expense_ai_quota ON expense_ai_usage(company_id,month);

CREATE UNIQUE INDEX expense_ai_source_active ON expense_ai_proposals(company_id,expense_id,COALESCE(payment_draft_id,''),stage,source_version,evidence_digest,provider_json)
 WHERE state IN ('queued','running','ready','applied');

CREATE INDEX employee_requests_company_queue ON employee_requests(company_id,state,updated_at);

CREATE INDEX employee_requests_requester ON employee_requests(company_id,requester_user_id,updated_at);

CREATE INDEX employee_requests_team ON employee_requests(company_id,team_id,state);

CREATE UNIQUE INDEX employee_requests_order ON employee_requests(company_id,purchase_order_id) WHERE purchase_order_id IS NOT NULL;

CREATE INDEX employee_request_history ON employee_request_events(request_id,created_at);

CREATE INDEX handbook_published ON handbook_articles(company_id,archived,published_revision);

CREATE INDEX request_ai_queue ON request_ai_proposals(state,claim_until,created_at);

CREATE INDEX request_ai_quota ON request_ai_usage(company_id,month);

CREATE FUNCTION organization_route_immutable_fn() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.slug <> OLD.slug
  OR NEW.kind <> OLD.kind
  OR NEW.parent_id IS DISTINCT FROM OLD.parent_id THEN RAISE EXCEPTION 'IMMUTABLE_ORGANIZATION_ROUTE' USING ERRCODE='23514'; END IF; RETURN NEW; END $$;
CREATE TRIGGER organization_route_immutable BEFORE UPDATE OF slug, parent_id, kind ON organization_nodes FOR EACH ROW EXECUTE FUNCTION organization_route_immutable_fn();

CREATE FUNCTION assignment_role_scope_insert_fn() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NOT ((NEW.role IN ('holding_owner','it_admin') AND NEW.scope_kind='holding') OR
 (NEW.role IN ('warehouse_manager','operations_manager') AND NEW.scope_kind IN ('holding','branch')) OR
 (NEW.role IN ('employee','finance_preparer','people_manager','hr_admin','payroll_preparer','payroll_approver','finance_approver','treasury','auditor','sales_manager','sales_operator','purchasing_manager','purchasing_operator') AND NEW.scope_kind IN ('holding','branch','department'))) THEN RAISE EXCEPTION 'INVALID_ROLE_SCOPE' USING ERRCODE='23514'; END IF; RETURN NEW; END $$;
CREATE TRIGGER assignment_role_scope_insert BEFORE INSERT ON assignments FOR EACH ROW EXECUTE FUNCTION assignment_role_scope_insert_fn();

CREATE FUNCTION assignment_role_scope_update_fn() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NOT ((NEW.role IN ('holding_owner','it_admin') AND NEW.scope_kind='holding') OR
 (NEW.role IN ('warehouse_manager','operations_manager') AND NEW.scope_kind IN ('holding','branch')) OR
 (NEW.role IN ('employee','finance_preparer','people_manager','hr_admin','payroll_preparer','payroll_approver','finance_approver','treasury','auditor','sales_manager','sales_operator','purchasing_manager','purchasing_operator') AND NEW.scope_kind IN ('holding','branch','department'))) THEN RAISE EXCEPTION 'INVALID_ROLE_SCOPE' USING ERRCODE='23514'; END IF; RETURN NEW; END $$;
CREATE TRIGGER assignment_role_scope_update BEFORE UPDATE OF role,scope_kind ON assignments FOR EACH ROW EXECUTE FUNCTION assignment_role_scope_update_fn();

CREATE FUNCTION social_comment_revision_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='UPDATE' THEN NEW.version=OLD.version+1; END IF;
 UPDATE social_posts SET version=version+1,updated_at=GREATEST(updated_at,NEW.updated_at) WHERE id=NEW.post_id;
 RETURN NEW;
END $$;
CREATE TRIGGER social_comment_insert_revision BEFORE INSERT ON social_comments FOR EACH ROW EXECUTE FUNCTION social_comment_revision_fn();
CREATE TRIGGER social_comment_update_revision BEFORE UPDATE OF body,state ON social_comments FOR EACH ROW EXECUTE FUNCTION social_comment_revision_fn();
ALTER TABLE outbox_events ADD COLUMN processed_at BIGINT;
ALTER TABLE outbox_events ADD COLUMN next_attempt_at BIGINT NOT NULL DEFAULT 0;
ALTER TABLE outbox_events ADD COLUMN lease_token TEXT;
ALTER TABLE outbox_events ADD COLUMN last_error TEXT;
ALTER TABLE outbox_events ADD COLUMN dead_letter_at BIGINT;
CREATE INDEX outbox_pending_jobs ON outbox_events(next_attempt_at,created_at) WHERE processed_at IS NULL AND dead_letter_at IS NULL;

