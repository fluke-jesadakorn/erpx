import 'server-only';
/** Fixed explicit-column owning projections; retains the consumer's supplied statement snapshot. */
export function identityVerifiedContextProjections(){return {
  "iam_store_authority_seats_s_from": "FROM (SELECT \"company_id\",\"kind\",\"scope_id\",\"version\",\"position_id\",\"person_id\",\"effective_at\" FROM authority_seats) s",
  "workflow_approvals_facts_authority_seats_s_from": "FROM (SELECT \"company_id\",\"kind\",\"scope_id\",\"version\",\"position_id\",\"person_id\",\"effective_at\" FROM authority_seats) s"
} as const;}
