import "server-only";
/** Closed owner read projections; consumers retain original filters, aliases and binds in their supplied statement/transaction. */
export const lastAuditOwnerProjectionSources = {
  marketing_campaign_costs: "SELECT \"id\",\"company_id\",\"campaign_id\",\"journal_line_id\",\"evidence\",\"reviewed_by\",\"created_at\" FROM marketing_campaign_costs",
} as const;
