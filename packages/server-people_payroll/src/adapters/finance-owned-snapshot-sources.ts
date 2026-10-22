import "server-only";
/** Closed native projections with explicit owned columns for named Finance reads.
 * The consumer retains its supplied transaction and one-statement snapshot;
 * this contract accepts no table name, predicate, SQL, or query from a caller. */
export function financeOwnedSnapshotSources(){return {
  "commerce_facts_hr_people_p_join": "JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p",
  "payroll_payslip_posting_hr_payroll_runs_r_join": "JOIN (SELECT \"id\",\"company_id\",\"pay_group_id\",\"country_pack_id\",\"period_start\",\"period_end\",\"pay_date\",\"currency\",\"mode\",\"status\",\"revision\",\"totals_json\",\"input_digest\",\"error_message\",\"version\",\"created_by\",\"approved_by\",\"posted_by\",\"created_at\",\"updated_at\",\"scope_engagement_id\",\"submitted_by\",\"submitted_digest\",\"submitted_at\",\"adjustment_of_run_id\" FROM hr_payroll_runs) r",
  "payroll_payslip_posting_hr_payroll_postings_p_from": "FROM (SELECT \"id\",\"run_id\",\"journal_id\",\"created_at\",\"account_mapping_id\",\"account_mapping_version\" FROM hr_payroll_postings) p",
  "payroll_remittance_sources_hr_payroll_postings_p_join": "JOIN (SELECT \"id\",\"run_id\",\"journal_id\",\"created_at\",\"account_mapping_id\",\"account_mapping_version\" FROM hr_payroll_postings) p",
  "payroll_remittance_sources_hr_payroll_runs_r_from": "FROM (SELECT \"id\",\"company_id\",\"pay_group_id\",\"country_pack_id\",\"period_start\",\"period_end\",\"pay_date\",\"currency\",\"mode\",\"status\",\"revision\",\"totals_json\",\"input_digest\",\"error_message\",\"version\",\"created_by\",\"approved_by\",\"posted_by\",\"created_at\",\"updated_at\",\"scope_engagement_id\",\"submitted_by\",\"submitted_digest\",\"submitted_at\",\"adjustment_of_run_id\" FROM hr_payroll_runs) r",
  "payroll_remittance_sources_hr_payroll_statutory_remittances_from": "FROM (SELECT \"id\",\"company_id\",\"run_id\",\"kind\",\"amount_minor\",\"liability_account_id\",\"bank_account_id\",\"bank_transaction_id\",\"journal_id\",\"bank_reference\",\"recorded_by\",\"recorded_at\" FROM hr_payroll_statutory_remittances) hr_payroll_statutory_remittances"
} as const;}
