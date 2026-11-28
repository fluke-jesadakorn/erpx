import "server-only";
import { FINANCE_ACCOUNTS_PROJECTION_SQL, FINANCE_INVOICES_PROJECTION_SQL, FINANCE_PARTIES_PROJECTION_SQL } from "./finance-tail-dedupe-sql";
import { FINANCE_JOURNAL_LINES_PROJECTION_SQL } from "./finance-journal-lines-projection-dedupe-sql";
import { FINANCE_JOURNALS_PROJECTION_SQL } from "./finance-journals-projection-dedupe-sql";
import { PAYMENT_PROJECTION_SQL, ALLOCATION_PROJECTION_SQL } from "./finance-read-tail-r18-sql";
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted. */
export const hospitalityOwnerProjectionSources = {
  finance_accounts: FINANCE_ACCOUNTS_PROJECTION_SQL,
  finance_ai_policies: "SELECT \"company_id\",\"enabled\",\"version\",\"kinds_json\",\"timezone\",\"daily_limit\",\"monthly_limit\",\"owner_actor_json\",\"reason\",\"changed_by\",\"changed_at\",\"next_scan_at\",\"last_error\" FROM finance_ai_policies",
  finance_ai_usage: "SELECT \"id\",\"proposal_id\",\"company_id\",\"actor_id\",\"month\",\"business_date\",\"state\",\"input_tokens\",\"output_tokens\",\"lease_until\",\"created_at\" FROM finance_ai_usage",
  finance_allocations: ALLOCATION_PROJECTION_SQL,
  finance_invoices: FINANCE_INVOICES_PROJECTION_SQL,
  finance_journal_lines: FINANCE_JOURNAL_LINES_PROJECTION_SQL,
  finance_journals: FINANCE_JOURNALS_PROJECTION_SQL,
  finance_parties: FINANCE_PARTIES_PROJECTION_SQL,
  finance_payments: PAYMENT_PROJECTION_SQL,
  finance_periods: "SELECT \"id\",\"company_id\",\"name\",\"start_date\",\"end_date\",\"state\",\"close_version\",\"closed_by\",\"closed_at\",\"created_at\",\"close_started_by\",\"reconciliation_hash\" FROM finance_periods",
} as const;
