import 'server-only';
import {legalMembersSnapshotSource} from './legal-members-read-sources';
/** Closed explicit-column owner projections for the supplied IAM statement snapshot.
 * No caller-supplied table, SQL, predicate or alias is accepted. */
export function identitySnapshotSources(){return {
  "legal_people_source_legal_members_m_from": legalMembersSnapshotSource
} as const;}
