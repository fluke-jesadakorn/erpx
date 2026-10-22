import 'server-only';
/** Fixed explicit-column owning projections; retains the consumer's supplied statement snapshot. */
export function identityVerifiedContextProjections(){return {
  "work_role_selection_facts_work_role_preferences_from": "FROM (SELECT \"user_id\",\"company_id\",\"assignment_id\",\"assignment_version\",\"updated_at\" FROM work_role_preferences) work_role_preferences"
} as const;}
