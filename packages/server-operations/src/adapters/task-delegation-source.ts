import 'server-only';
/** Canonical source projections shared by the Operations task image SQL clauses. */
export const OPERATIONS_TASK_DELEGATION_SOURCE_SQL = "(SELECT jsonb_agg(to_jsonb(delegation) ORDER BY delegation.id) FROM staff_task_delegations delegation WHERE delegation.task_id=t.id AND delegation.company_id=t.company_id)";
/** Canonical routine-template projection shared by the Operations task image SQL clauses. */
export const OPERATIONS_TASK_TEMPLATE_SOURCE_SQL = "(SELECT to_jsonb(template) FROM routine_templates template WHERE template.id=t.template_id AND template.company_id=t.company_id)";
/** Canonical routine-schedule projection shared by the Operations task image SQL clauses. */
export const OPERATIONS_TASK_SCHEDULE_SOURCE_SQL = "(SELECT to_jsonb(schedule) FROM routine_schedules schedule WHERE schedule.id=t.schedule_id AND schedule.company_id=t.company_id)";
