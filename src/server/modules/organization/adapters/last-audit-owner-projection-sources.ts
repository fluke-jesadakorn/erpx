import "server-only";
/** Closed owner read projections; consumers retain original filters, aliases and binds in their supplied statement/transaction. */
export const lastAuditOwnerProjectionSources = {
  spending_approval_templates: "SELECT \"company_id\",\"version\",\"exceptions_json\",\"reason\",\"updated_by\",\"updated_at\" FROM spending_approval_templates",
} as const;
