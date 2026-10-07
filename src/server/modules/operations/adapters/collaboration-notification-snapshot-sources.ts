import 'server-only';
/** Closed, named source clauses for the supplied notification statement snapshot.
 * No table, SQL, predicate or alias is accepted from a caller. */
export function collaborationNotificationSnapshotSources(){return {
  "operations_handover_facts_staff_tasks_t_from": "FROM (SELECT \"id\", \"company_id\", \"branch_id\", \"department_id\", \"title\", \"description\", \"creator_user_id\", \"assignee_user_id\", \"due_at\", \"project_id\", \"completion_note\", \"start_approvers_json\", \"completion_approvers_json\", \"state\", \"blocked_reason\", \"revision\", \"version\", \"template_id\", \"template_version\", \"schedule_id\", \"occurrence_at\", \"created_at\", \"updated_at\" FROM staff_tasks) t"
} as const;}
