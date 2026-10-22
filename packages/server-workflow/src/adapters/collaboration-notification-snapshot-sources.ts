import 'server-only';
/** Closed, named source clauses for the supplied notification statement snapshot.
 * No table, SQL, predicate or alias is accepted from a caller. */
export function collaborationNotificationSnapshotSources(){return {
  "workflow_notification_effects_flow_routes_r_join": "JOIN (SELECT \"company_id\", \"kind\", \"record_id\", \"version\", \"defaults_revision\", \"steps_json\", \"created_at\" FROM flow_routes) r"
} as const;}
