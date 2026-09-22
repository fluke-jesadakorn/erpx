CREATE TABLE finance_calculation_policies (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, kind TEXT NOT NULL, name TEXT NOT NULL,
  country_code TEXT, currency TEXT, effective_from TEXT NOT NULL, effective_to TEXT,
  definition_json TEXT NOT NULL CHECK(erpx_json_valid(definition_json)), status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','approved','retired')),
  version BIGINT NOT NULL DEFAULT 1, created_by TEXT NOT NULL, approved_by TEXT, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
  UNIQUE(company_id,kind,name,effective_from)
);
CREATE TABLE finance_calculation_runs (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, holding_id TEXT NOT NULL, kind TEXT NOT NULL CHECK(kind IN ('allocation','cash_flow','consolidation','thai_statutory','manufacturing_cost','reconciliation')),
  policy_id TEXT REFERENCES finance_calculation_policies(id), policy_version BIGINT NOT NULL, period_id TEXT REFERENCES finance_periods(id), currency TEXT NOT NULL, currency_scale BIGINT NOT NULL CHECK(currency_scale BETWEEN 0 AND 12),
  source_hash TEXT NOT NULL, input_json TEXT NOT NULL CHECK(erpx_json_valid(input_json)), result_json TEXT NOT NULL CHECK(erpx_json_valid(result_json)), explanation_json TEXT NOT NULL CHECK(erpx_json_valid(explanation_json)),
  status TEXT NOT NULL DEFAULT 'prepared' CHECK(status IN ('prepared','reviewed','approved','stale','posted','published','rejected')),
  version BIGINT NOT NULL DEFAULT 1, prepared_by TEXT NOT NULL, reviewed_by TEXT, approved_by TEXT, journal_id TEXT REFERENCES finance_journals(id), created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
);
CREATE INDEX finance_calculation_runs_company_kind_idx ON finance_calculation_runs(company_id,kind,status,created_at);
CREATE TABLE finance_calculation_events (
  id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES finance_calculation_runs(id), version BIGINT NOT NULL, action TEXT NOT NULL, actor_id TEXT NOT NULL, reason TEXT NOT NULL,
  result_hash TEXT NOT NULL, created_at BIGINT NOT NULL, UNIQUE(run_id,version)
);
CREATE TABLE finance_cash_flow_mappings (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, account_id TEXT NOT NULL REFERENCES finance_accounts(id), activity TEXT NOT NULL CHECK(activity IN ('cash','operating','investing','financing','noncash','unmapped')),
  adjustment_kind TEXT, policy_version BIGINT NOT NULL, effective_from TEXT NOT NULL, effective_to TEXT, created_by TEXT NOT NULL, created_at BIGINT NOT NULL,
  UNIQUE(company_id,account_id,policy_version)
);
CREATE TABLE finance_statutory_packages (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, period_id TEXT REFERENCES finance_periods(id), jurisdiction TEXT NOT NULL DEFAULT 'TH', form_code TEXT NOT NULL,
  filing_kind TEXT NOT NULL CHECK(filing_kind IN ('original','additional')), calculation_run_id TEXT NOT NULL REFERENCES finance_calculation_runs(id), status TEXT NOT NULL CHECK(status IN ('prepared','reviewed','approved_for_export','exported','filed','paid','superseded')),
  schema_version TEXT NOT NULL, effective_date TEXT NOT NULL, export_hash TEXT, filing_reference TEXT, evidence_json TEXT NOT NULL DEFAULT '{}' CHECK(erpx_json_valid(evidence_json)), version BIGINT NOT NULL DEFAULT 1,
  prepared_by TEXT NOT NULL, reviewed_by TEXT, approved_by TEXT, filed_at BIGINT, paid_at BIGINT, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
);
CREATE TABLE finance_group_entities (
  id TEXT PRIMARY KEY, holding_id TEXT NOT NULL, company_id TEXT NOT NULL, reporting_name TEXT NOT NULL, functional_currency TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'active', created_at BIGINT NOT NULL, UNIQUE(holding_id,company_id)
);
CREATE TABLE finance_ownership_interests (
  id TEXT PRIMARY KEY, holding_id TEXT NOT NULL, investor_entity_id TEXT NOT NULL REFERENCES finance_group_entities(id), investee_entity_id TEXT NOT NULL REFERENCES finance_group_entities(id),
  accounting_method TEXT NOT NULL CHECK(accounting_method IN ('subsidiary','equity','cost','excluded')), ownership_bps BIGINT NOT NULL CHECK(ownership_bps BETWEEN 0 AND 10000), voting_bps BIGINT NOT NULL CHECK(voting_bps BETWEEN 0 AND 10000), control_assessment TEXT NOT NULL,
  effective_from TEXT NOT NULL, effective_to TEXT, status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','approved','retired')), version BIGINT NOT NULL DEFAULT 1, created_by TEXT NOT NULL, approved_by TEXT, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
);
CREATE TABLE finance_acquisitions (
  id TEXT PRIMARY KEY, holding_id TEXT NOT NULL, investee_entity_id TEXT NOT NULL REFERENCES finance_group_entities(id), acquisition_date TEXT NOT NULL, currency TEXT NOT NULL,
  consideration_minor BIGINT NOT NULL, previous_interest_minor BIGINT NOT NULL DEFAULT 0, nci_measurement TEXT NOT NULL CHECK(nci_measurement IN ('fair_value','proportionate')), nci_minor BIGINT NOT NULL,
  identifiable_net_assets_minor BIGINT NOT NULL, deferred_tax_minor BIGINT NOT NULL DEFAULT 0, goodwill_minor BIGINT NOT NULL DEFAULT 0, bargain_purchase_minor BIGINT NOT NULL DEFAULT 0,
  cgu_reference TEXT, calculation_run_id TEXT REFERENCES finance_calculation_runs(id), status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','reviewed','approved','impaired')), version BIGINT NOT NULL DEFAULT 1, created_by TEXT NOT NULL, approved_by TEXT, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
);
CREATE TABLE finance_bank_match_groups (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, currency TEXT NOT NULL, status TEXT NOT NULL CHECK(status IN ('proposed','matched','reversed','rejected')), source_hash TEXT NOT NULL, explanation_json TEXT NOT NULL CHECK(erpx_json_valid(explanation_json)),
  proposed_by TEXT NOT NULL, confirmed_by TEXT, reversed_by TEXT, version BIGINT NOT NULL DEFAULT 1, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
);
CREATE TABLE finance_bank_match_allocations (
  id TEXT PRIMARY KEY, group_id TEXT NOT NULL REFERENCES finance_bank_match_groups(id), bank_transaction_id TEXT NOT NULL REFERENCES finance_bank_transactions(id), target_type TEXT NOT NULL, target_id TEXT NOT NULL,
  amount_minor BIGINT NOT NULL CHECK(amount_minor>0), created_at BIGINT NOT NULL, UNIQUE(group_id,bank_transaction_id,target_type,target_id)
);
CREATE TABLE finance_close_checks (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, period_id TEXT NOT NULL REFERENCES finance_periods(id), check_key TEXT NOT NULL, source_version TEXT NOT NULL, status TEXT NOT NULL CHECK(status IN ('pass','block','warning','waived')),
  count_value BIGINT NOT NULL DEFAULT 0, amount_minor BIGINT, detail_json TEXT NOT NULL CHECK(erpx_json_valid(detail_json)), owner_id TEXT, evidence_json TEXT NOT NULL DEFAULT '{}' CHECK(erpx_json_valid(evidence_json)),
  checked_at BIGINT NOT NULL, checked_by TEXT NOT NULL, resolved_at BIGINT, resolved_by TEXT, UNIQUE(period_id,check_key,source_version)
);
CREATE TABLE manufacturing_cost_movements (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, order_id TEXT NOT NULL REFERENCES manufacturing_orders(id), movement_kind TEXT NOT NULL CHECK(movement_kind IN ('material_issue','material_return','labor','overhead','output','normal_scrap','abnormal_scrap','adjustment')),
  quantity_milli BIGINT NOT NULL DEFAULT 0, amount_minor BIGINT NOT NULL, source_type TEXT NOT NULL, source_id TEXT NOT NULL, calculation_run_id TEXT REFERENCES finance_calculation_runs(id), journal_id TEXT REFERENCES finance_journals(id), occurred_on TEXT NOT NULL,
  version BIGINT NOT NULL DEFAULT 1, created_by TEXT NOT NULL, created_at BIGINT NOT NULL, UNIQUE(order_id,movement_kind,source_type,source_id)
);
CREATE TABLE manufacturing_cost_pools (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, period_id TEXT NOT NULL REFERENCES finance_periods(id), name TEXT NOT NULL, allocation_basis TEXT NOT NULL CHECK(allocation_basis IN ('labor_hours','machine_hours','material_cost','output_quantity','manual')),
  amount_minor BIGINT NOT NULL, source_hash TEXT NOT NULL, status TEXT NOT NULL CHECK(status IN ('draft','reviewed','allocated','reversed')), version BIGINT NOT NULL DEFAULT 1, created_by TEXT NOT NULL, reviewed_by TEXT, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
);
CREATE TABLE manufacturing_cost_allocations (
  id TEXT PRIMARY KEY, pool_id TEXT NOT NULL REFERENCES manufacturing_cost_pools(id), order_id TEXT NOT NULL REFERENCES manufacturing_orders(id), basis_quantity_milli BIGINT NOT NULL, allocated_minor BIGINT NOT NULL, calculation_run_id TEXT NOT NULL REFERENCES finance_calculation_runs(id), created_at BIGINT NOT NULL, UNIQUE(pool_id,order_id)
);
DO $$ DECLARE t TEXT; BEGIN FOREACH t IN ARRAY ARRAY['finance_calculation_policies','finance_calculation_runs','finance_statutory_packages','finance_group_entities','finance_ownership_interests','finance_acquisitions','finance_bank_match_groups','finance_close_checks','manufacturing_cost_movements','manufacturing_cost_pools'] LOOP INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING; EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t); EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t); END LOOP; END $$;
