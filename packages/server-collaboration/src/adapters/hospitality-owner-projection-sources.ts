import "server-only";
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted. */
export const hospitalityOwnerProjectionSources = {
  social_ai_usage: "SELECT \"id\",\"company_id\",\"actor_id\",\"month\",\"state\",\"input_tokens\",\"output_tokens\",\"lease_until\",\"created_at\" FROM social_ai_usage",
} as const;
