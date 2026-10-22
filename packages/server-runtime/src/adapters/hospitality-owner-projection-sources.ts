import "server-only";
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted. */
export const hospitalityOwnerProjectionSources = {
  executive_generation_jobs: "SELECT \"id\",\"report_id\",\"company_id\",\"user_id\",\"assignment_id\",\"scope_key\",\"question\",\"input_json\",\"actor_json\",\"provider_json\",\"state\",\"proposal_json\",\"error\",\"lease_token\",\"lease_until\",\"attempts\",\"created_at\",\"updated_at\" FROM executive_generation_jobs",
} as const;
