import 'server-only';
/** Canonical active-delegation projection shared by Operations task source clauses. */
export const OPERATIONS_TASK_DELEGATION_SOURCE_SQL = "(SELECT jsonb_agg(to_jsonb(delegation) ORDER BY delegation.id) FROM staff_task_delegations delegation WHERE delegation.task_id=t.id AND delegation.company_id=t.company_id)";
