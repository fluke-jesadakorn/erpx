import "server-only";
/** Closed native projections for commercial owner joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const commercialOwnerProjectionSources = {
  marketing_campaign_costs: "SELECT \"id\",\"company_id\",\"campaign_id\",\"journal_line_id\",\"evidence\",\"reviewed_by\",\"created_at\" FROM marketing_campaign_costs",
  marketing_campaigns: "SELECT \"id\",\"company_id\",\"code\",\"name\",\"created_by\",\"created_at\",\"version\" FROM marketing_campaigns",
  marketing_lead_candidates: "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"form_id\",\"form_version\",\"contact_name\",\"contact_detail\",\"consent_evidence\",\"attribution_json\",\"brief\",\"status\",\"crm_lead_id\",\"created_by\",\"reviewed_by\",\"version\",\"created_at\",\"updated_at\" FROM marketing_lead_candidates",
  marketing_touches: "SELECT \"id\",\"company_id\",\"lead_id\",\"campaign_id\",\"source\",\"evidence\",\"source_kind\",\"source_id\",\"occurred_at\",\"created_by\" FROM marketing_touches",
} as const;
