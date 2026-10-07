import 'server-only';
/** Closed explicit-column owner projections for the supplied IAM statement snapshot.
 * No caller-supplied table, SQL, predicate or alias is accepted. */
export function identitySnapshotSources(){return {
  "records_employee_requests_facts_service_team_members_m_from": "FROM (SELECT \"company_id\", \"team_id\", \"user_id\", \"responsibility\" FROM service_team_members) m"
} as const;}
