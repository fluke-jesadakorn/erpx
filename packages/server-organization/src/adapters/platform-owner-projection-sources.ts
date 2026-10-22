import "server-only";
/** Closed owner projections for Platform, Organization and Procurement joins. Original filters, bindings and supplied transactions remain with the caller. */
export const platformOwnerProjectionSources = Object.freeze({
  company_business_acceptance: "SELECT \"company_id\",\"status\",\"version\" FROM company_business_acceptance",
  organization_nodes: "SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes",
} as const);
