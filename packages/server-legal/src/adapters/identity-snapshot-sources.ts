import 'server-only';
/** Closed explicit-column owner projections for the supplied IAM statement snapshot.
 * No caller-supplied table, SQL, predicate or alias is accepted. */
export function identitySnapshotSources(){return {
  "legal_people_source_legal_members_m_from": "FROM (SELECT \"item_id\", \"user_id\", \"access\" FROM legal_members) m"
} as const;}
