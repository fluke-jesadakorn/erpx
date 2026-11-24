import 'server-only';
/** Canonical full-row staff_tasks projection shared by Operations snapshot sources. */
export const OPERATIONS_STAFF_TASKS_SOURCE_SQL = 'SELECT "id", "company_id", "branch_id", "department_id", "title", "description", "creator_user_id", "assignee_user_id", "due_at", "project_id", "completion_note", "start_approvers_json", "completion_approvers_json", "state", "blocked_reason", "revision", "version", "template_id", "template_version", "schedule_id", "occurrence_at", "created_at", "updated_at" FROM staff_tasks';
export function identitySnapshotSources(){return {
  "operations_staff_tasks_facts_staff_tasks_t_from": `FROM (${OPERATIONS_STAFF_TASKS_SOURCE_SQL}) t`
} as const;}
