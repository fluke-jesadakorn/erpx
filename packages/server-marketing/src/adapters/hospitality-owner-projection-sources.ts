import "server-only";
/** Owner-local fixed projection for Marketing campaign evidence. The native
 * executive/campaign readers return this exact shape; one canonical source
 * keeps the hospitality and commercial projections from drifting apart. */
export const marketingCampaignProjection = "SELECT \"id\",\"company_id\",\"code\",\"name\",\"created_by\",\"created_at\",\"version\" FROM marketing_campaigns";
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted. */
export const hospitalityOwnerProjectionSources = {
  marketing_campaigns: marketingCampaignProjection,
} as const;
