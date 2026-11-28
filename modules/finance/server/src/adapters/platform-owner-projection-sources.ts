import "server-only";
import { FINANCE_ACCOUNTS_PROJECTION_SQL, FINANCE_INVOICES_PROJECTION_SQL, FINANCE_PARTIES_PROJECTION_SQL } from "./finance-tail-dedupe-sql";
import { FINANCE_JOURNALS_PROJECTION_SQL } from "./finance-journals-projection-dedupe-sql";
import { FINANCE_CHARTS_PROJECTION_SQL } from "./finance-charts-projection-dedupe-sql";
import { FINANCE_JOURNAL_LINES_PROJECTION_SQL } from "./finance-journal-lines-projection-dedupe-sql";
/** Closed owner projections for Platform, Organization and Procurement joins. Original filters, bindings and supplied transactions remain with the caller. */
export const platformOwnerProjectionSources = Object.freeze({
  finance_accounts: FINANCE_ACCOUNTS_PROJECTION_SQL,
  finance_charts: FINANCE_CHARTS_PROJECTION_SQL,
  finance_invoices: FINANCE_INVOICES_PROJECTION_SQL,
  finance_journal_lines: FINANCE_JOURNAL_LINES_PROJECTION_SQL,
  finance_journals: FINANCE_JOURNALS_PROJECTION_SQL,
  finance_parties: FINANCE_PARTIES_PROJECTION_SQL,
  finance_supplier_batches: "SELECT \"id\",\"company_id\",\"currency\",\"bank_account_id\",\"planned_date\",\"status\",\"created_by\",\"approved_by\",\"approval_reason\",\"exported_by\",\"export_command_id\",\"export_hash\",\"version\",\"created_at\",\"updated_at\" FROM finance_supplier_batches",
  finance_supplier_instructions: "SELECT \"id\",\"batch_id\",\"invoice_id\",\"invoice_version\",\"amount_minor\",\"beneficiary_name\",\"beneficiary_account\",\"status\",\"evidence\",\"resolved_by\",\"bank_reference\",\"payment_id\",\"bank_transaction_id\",\"version\",\"created_at\",\"updated_at\" FROM finance_supplier_instructions",
} as const);
