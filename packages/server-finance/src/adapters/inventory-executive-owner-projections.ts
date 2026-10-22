import "server-only";
/** Closed native projections for Inventory, Planning, Manufacturing and Project snapshots. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const inventoryExecutiveOwnerProjections = Object.freeze({
  finance_accounts: "SELECT \"id\",\"chart_id\",\"company_id\",\"code\",\"name\",\"category\",\"parent_id\",\"currency\",\"is_control\",\"status\",\"version\",\"created_at\",\"updated_at\",\"control_kind\" FROM finance_accounts",
  finance_charts: "SELECT \"id\",\"company_id\",\"name\",\"base_currency\",\"accounting_basis\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\" FROM finance_charts",
  finance_journal_lines: "SELECT \"id\",\"journal_id\",\"account_id\",\"description\",\"debit_minor\",\"credit_minor\",\"base_debit_minor\",\"base_credit_minor\",\"dimension_json\",\"ordinal\" FROM finance_journal_lines",
  finance_journals: "SELECT \"id\",\"company_id\",\"chart_id\",\"period_id\",\"journal_number\",\"entry_date\",\"currency\",\"fx_rate_micros\",\"source_type\",\"source_id\",\"memo\",\"status\",\"is_opening\",\"version\",\"created_by\",\"posted_by\",\"posted_at\",\"reversed_by\",\"reversed_at\",\"created_at\",\"updated_at\" FROM finance_journals",
} as const);
