import 'server-only';
/** Closed, named source clauses for the supplied notification statement snapshot.
 * No table, SQL, predicate or alias is accepted from a caller. */
export function collaborationNotificationSnapshotSources(){return {
  "maintenance_notifications_maintenance_work_orders_from": "FROM (SELECT \"id\", \"company_id\", \"branch_id\", \"department_id\", \"asset_id\", \"title\", \"priority\", \"status\", \"owner_id\", \"scheduled_at\", \"downtime_started_at\", \"downtime_ended_at\", \"blocked_reason\", \"completion_evidence\", \"resulting_condition\", \"version\", \"created_by\", \"created_at\", \"updated_at\" FROM maintenance_work_orders) maintenance_work_orders",
  "maintenance_notifications_maintenance_preventive_plans_from": "FROM (SELECT \"id\", \"company_id\", \"branch_id\", \"department_id\", \"asset_id\", \"title\", \"priority\", \"owner_id\", \"interval_days\", \"time_zone\", \"first_due_local\", \"next_due_local\", \"next_due_at\", \"lead_days\", \"status\", \"version\", \"created_by\", \"edited_by\", \"approved_by\", \"approved_at\", \"created_at\", \"updated_at\" FROM maintenance_preventive_plans) maintenance_preventive_plans"
} as const;}
