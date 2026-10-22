import 'server-only';
/** Fixed explicit-column owning projections; retains the consumer's supplied statement snapshot. */
export function identityVerifiedContextProjections(){return {
  "operations_routines_facts_routine_templates_t_from": "FROM (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"title\",\"description\",\"creator_user_id\",\"assignee_user_id\",\"project_id\",\"start_approvers_json\",\"completion_approvers_json\",\"published\",\"version\",\"created_at\",\"updated_at\" FROM routine_templates) t"
} as const;}
