import "server-only";
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted. */
export const hospitalityOwnerProjectionSources = {
  beta_policy: "SELECT \"company_id\",\"member_limit\",\"branch_limit\",\"ai_monthly_limit\",\"storage_bytes\",\"created_at\" FROM beta_policy",
} as const;
