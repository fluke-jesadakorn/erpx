import "server-only";
import { FINANCE_ACCOUNTS_PROJECTION_SQL, FINANCE_INVOICES_PROJECTION_SQL, FINANCE_PARTIES_PROJECTION_SQL } from "./finance-tail-dedupe-sql";
import { FINANCE_CHARTS_PROJECTION_SQL } from "./finance-charts-projection-dedupe-sql";
import { FINANCE_JOURNAL_LINES_PROJECTION_SQL } from "./finance-journal-lines-projection-dedupe-sql";
import { FINANCE_JOURNALS_PROJECTION_SQL } from "./finance-journals-projection-dedupe-sql";
/** Closed native projections for commercial owner joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const commercialOwnerProjectionSources = {
  finance_accounts: FINANCE_ACCOUNTS_PROJECTION_SQL,
  finance_allocations: "SELECT \"id\",\"company_id\",\"payment_id\",\"invoice_id\",\"amount_minor\",\"created_by\",\"created_at\",\"base_invoice_minor\",\"base_payment_minor\",\"realized_fx_minor\" FROM finance_allocations",
  finance_charts: FINANCE_CHARTS_PROJECTION_SQL,
  finance_invoices: FINANCE_INVOICES_PROJECTION_SQL,
  finance_journal_lines: FINANCE_JOURNAL_LINES_PROJECTION_SQL,
  finance_journals: FINANCE_JOURNALS_PROJECTION_SQL,
  finance_parties: FINANCE_PARTIES_PROJECTION_SQL,
  finance_refund_obligations: "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"return_id\",\"invoice_id\",\"party_id\",\"direction\",\"currency\",\"chart_id\",\"balance_account_id\",\"amount_minor\",\"outstanding_minor\",\"status\",\"beneficiary_name\",\"beneficiary_account\",\"approval_reason\",\"created_by\",\"approved_by\",\"approved_at\",\"version\",\"created_at\",\"updated_at\",\"base_amount_minor\",\"base_outstanding_minor\",\"source_credit_journal_id\",\"billing_correction_id\" FROM finance_refund_obligations",
} as const;
