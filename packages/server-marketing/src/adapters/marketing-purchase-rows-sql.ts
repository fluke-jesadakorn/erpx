import "server-only";
import { crmMarketingCandidateSources } from "./crm-purchase-sources";

/** Owner-local fixed witness-row projections for Marketing purchase reads.
 * The four admitted variants share one branch/department/witness frame; callers
 * pick a variant and never author query text, so no duplicate projection
 * literal survives in the owner. Each variant emits the original statement
 * byte for byte, including the captured candidate source and join ordering. */
export const marketingPurchaseRowsSql = (
  variant: "asset" | "candidate" | "campaign" | "campaign-cost",
): string => {
  const spec = {
    asset: {
      scope: "NULL::text branch_id,NULL::text department_id",
      witness: "(to_jsonb(asset)-'status'-'reviewed_by'-'version'-'updated_at')",
      source: "marketing_assets asset",
    },
    candidate: {
      scope: "candidate.branch_id,candidate.department_id",
      witness: "(to_jsonb(candidate)-'status'-'crm_lead_id'-'reviewed_by'-'version'-'updated_at')",
      source: `(${crmMarketingCandidateSources}) candidate`,
    },
    campaign: {
      scope: "NULL::text branch_id,NULL::text department_id",
      witness: "to_jsonb(campaign)",
      source: "marketing_campaigns campaign",
    },
    "campaign-cost": {
      scope: "NULL::text branch_id,NULL::text department_id",
      witness: "to_jsonb(cost)",
      source: "marketing_campaign_costs cost JOIN marketing_campaigns campaign ON campaign.id=cost.campaign_id AND campaign.company_id=cost.company_id",
    },
  }[variant];
  const id = variant === "campaign-cost" ? "cost" : variant;
  return `SELECT ${id}.id,${id}.company_id,${spec.scope},${spec.witness} witness FROM ${spec.source}`;
};
