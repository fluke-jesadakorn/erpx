import "server-only";
import { marketingCampaignProjection } from "@erpx/server-marketing/adapters/hospitality-owner-projection-sources";
import { marketingCampaignCostProjection } from "@erpx/server-marketing/adapters/last-audit-owner-projection-sources";
import { marketingProjectionSourceSql } from "./owner-projection-source";
/** Closed native projections for commercial owner joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const commercialOwnerProjectionSources = {
  marketing_campaign_costs: marketingCampaignCostProjection,
  marketing_campaigns: marketingCampaignProjection,
  marketing_lead_candidates: marketingProjectionSourceSql(
    ["id","company_id","branch_id","department_id","form_id","form_version","contact_name","contact_detail","consent_evidence","attribution_json","brief","status","crm_lead_id","created_by","reviewed_by","version","created_at","updated_at"],
    "marketing_lead_candidates",
  ),
  marketing_touches: marketingProjectionSourceSql(
    ["id","company_id","lead_id","campaign_id","source","evidence","source_kind","source_id","occurred_at","created_by"],
    "marketing_touches",
  ),
} as const;
