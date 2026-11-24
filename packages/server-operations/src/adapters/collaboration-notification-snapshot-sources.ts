import 'server-only';
import {OPERATIONS_STAFF_TASKS_SOURCE_SQL} from './identity-snapshot-sources';
/** Closed, named source clauses for the supplied notification statement snapshot.
 * No table, SQL, predicate or alias is accepted from a caller. */
export function collaborationNotificationSnapshotSources(){return {
  "operations_handover_facts_staff_tasks_t_from": `FROM (${OPERATIONS_STAFF_TASKS_SOURCE_SQL}) t`
} as const;}
