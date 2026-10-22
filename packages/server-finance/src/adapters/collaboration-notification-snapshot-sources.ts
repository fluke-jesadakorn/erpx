import 'server-only';
/** Closed, named source clauses for the supplied notification statement snapshot.
 * No table, SQL, predicate or alias is accepted from a caller. */
export function collaborationNotificationSnapshotSources(){return {
  "workflow_notification_effects_finance_journals_j_join": "JOIN (SELECT \"id\", \"company_id\", \"chart_id\", \"period_id\", \"journal_number\", \"entry_date\", \"currency\", \"fx_rate_micros\", \"source_type\", \"source_id\", \"memo\", \"status\", \"is_opening\", \"version\", \"created_by\", \"posted_by\", \"posted_at\", \"reversed_by\", \"reversed_at\", \"created_at\", \"updated_at\" FROM finance_journals) j"
} as const;}
