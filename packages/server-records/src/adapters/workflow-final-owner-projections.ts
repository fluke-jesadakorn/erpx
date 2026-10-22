import 'server-only';
/** Fixed explicit-column owning projections; retains the consumer's supplied statement snapshot. */
export function workflowFinalOwnerProjections(){return {
  "finance_ai_request_sources_request_ai_usage_from": "FROM (SELECT \"id\",\"company_id\",\"actor_id\",\"month\",\"state\",\"lease_until\",\"input_tokens\",\"output_tokens\",\"created_at\" FROM request_ai_usage) request_ai_usage"
} as const;}
