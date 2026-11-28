import 'server-only';
import {flowRoutesClosedProjectionSql} from './closed-projection-sources';
/** Closed, named source clauses for the supplied notification statement snapshot.
 * No table, SQL, predicate or alias is accepted from a caller. */
export function collaborationNotificationSnapshotSources(){return {
  "workflow_notification_effects_flow_routes_r_join": `JOIN (${flowRoutesClosedProjectionSql(', ')}) r`
} as const;}
