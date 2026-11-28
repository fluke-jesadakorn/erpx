import "server-only";
import { marketingAnalyticsRowsSql } from "./analytics-null-scope-rows";

/** Closed-variant owner-local campaign projection for Marketing CRM analytics.
 * The enforced and legacy analytics readers share one fixed projection frame:
 * a caller-chosen projection and alias over the owner-owned campaign table and
 * null branch/department columns, now rendered by the shared NULL-scope head.
 * No raw query text is accepted from a caller, so no duplicate projection literal
 * survives in the owner and each admitted variant emits the original statement
 * byte for byte. */
export function marketingAnalyticsCampaignSql(enforced: boolean): string {
  const projection = enforced ? "campaign.*" : "*";
  const alias = enforced ? " campaign" : "";
  return marketingAnalyticsRowsSql(projection, `marketing_campaigns${alias}`);
}
