import "server-only";
/** Owner-local fixed projection for Marketing campaign-cost evidence. The native
 * executive/cost readers return this exact shape; one canonical source keeps
 * the last-audit and commercial projections from drifting apart. */
export const marketingCampaignCostProjection = "SELECT \"id\",\"company_id\",\"campaign_id\",\"journal_line_id\",\"evidence\",\"reviewed_by\",\"created_at\" FROM marketing_campaign_costs";
/** Closed owner read projections; consumers retain original filters, aliases and binds in their supplied statement/transaction. */
export const lastAuditOwnerProjectionSources = {
  marketing_campaign_costs: marketingCampaignCostProjection,
} as const;
