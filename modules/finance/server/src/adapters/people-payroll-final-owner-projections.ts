import 'server-only';
import { FINANCE_JOURNALS_JOIN_SQL } from './finance-read-tail-r15-sql';
/** Fixed explicit-column owning projections; retains the consumer's supplied statement snapshot. */
export function peoplePayrollFinalOwnerProjections(){return {
  "executive_main_evidence_sources_finance_journals_j_join": FINANCE_JOURNALS_JOIN_SQL,
  "executive_metric_projections_finance_journals_j_join": FINANCE_JOURNALS_JOIN_SQL
} as const;}
