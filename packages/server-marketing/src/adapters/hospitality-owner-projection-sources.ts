import "server-only";
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted. */
export const hospitalityOwnerProjectionSources = {
  marketing_campaigns: "SELECT \"id\",\"company_id\",\"code\",\"name\",\"created_by\",\"created_at\",\"version\" FROM marketing_campaigns",
} as const;
