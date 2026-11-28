import "server-only";
import { FINANCE_ACCOUNTS_PROJECTION_SQL, FINANCE_INVOICES_PROJECTION_SQL, FINANCE_PARTIES_PROJECTION_SQL } from "./finance-tail-dedupe-sql";
import { FINANCE_CHARTS_PROJECTION_SQL } from "./finance-charts-projection-dedupe-sql";
import { FINANCE_JOURNAL_LINES_PROJECTION_SQL } from "./finance-journal-lines-projection-dedupe-sql";
import { FINANCE_JOURNALS_PROJECTION_SQL } from "./finance-journals-projection-dedupe-sql";
import { REFUND_OBLIGATION_PROJECTION_SQL, ALLOCATION_PROJECTION_SQL } from "./finance-read-tail-r18-sql";
/** Closed native projections for commercial owner joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const commercialOwnerProjectionSources = {
  finance_accounts: FINANCE_ACCOUNTS_PROJECTION_SQL,
  finance_allocations: ALLOCATION_PROJECTION_SQL,
  finance_charts: FINANCE_CHARTS_PROJECTION_SQL,
  finance_invoices: FINANCE_INVOICES_PROJECTION_SQL,
  finance_journal_lines: FINANCE_JOURNAL_LINES_PROJECTION_SQL,
  finance_journals: FINANCE_JOURNALS_PROJECTION_SQL,
  finance_parties: FINANCE_PARTIES_PROJECTION_SQL,
  finance_refund_obligations: REFUND_OBLIGATION_PROJECTION_SQL,
} as const;
