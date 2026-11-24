import "server-only";
import { FINANCE_JOURNALS_PROJECTION_SQL } from "./finance-journals-projection-dedupe-sql";
import { FINANCE_CHARTS_PROJECTION_SQL } from "./finance-charts-projection-dedupe-sql";
/** Closed native projections for Inventory, Planning, Manufacturing and Project snapshots. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const inventoryExecutiveOwnerProjections = Object.freeze({
  finance_accounts: "SELECT \"id\",\"chart_id\",\"company_id\",\"code\",\"name\",\"category\",\"parent_id\",\"currency\",\"is_control\",\"status\",\"version\",\"created_at\",\"updated_at\",\"control_kind\" FROM finance_accounts",
  finance_charts: FINANCE_CHARTS_PROJECTION_SQL,
  finance_journal_lines: "SELECT \"id\",\"journal_id\",\"account_id\",\"description\",\"debit_minor\",\"credit_minor\",\"base_debit_minor\",\"base_credit_minor\",\"dimension_json\",\"ordinal\" FROM finance_journal_lines",
  finance_journals: FINANCE_JOURNALS_PROJECTION_SQL,
} as const);
