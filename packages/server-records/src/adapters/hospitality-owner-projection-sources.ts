import "server-only";
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted. */
export const hospitalityOwnerProjectionSources = {
  request_ai_usage: "SELECT \"id\",\"company_id\",\"actor_id\",\"month\",\"state\",\"lease_until\",\"input_tokens\",\"output_tokens\",\"created_at\" FROM request_ai_usage",
} as const;
