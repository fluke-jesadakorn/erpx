import "server-only";
/** Closed native projections for Legal, Asset, Maintenance and Customer joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const finalRootOwnerProjectionSources = {
  executive_generation_jobs: "SELECT \"id\",\"report_id\",\"company_id\",\"user_id\",\"assignment_id\",\"scope_key\",\"question\",\"input_json\",\"actor_json\",\"provider_json\",\"state\",\"proposal_json\",\"error\",\"lease_token\",\"lease_until\",\"attempts\",\"created_at\",\"updated_at\" FROM executive_generation_jobs",
} as const;
