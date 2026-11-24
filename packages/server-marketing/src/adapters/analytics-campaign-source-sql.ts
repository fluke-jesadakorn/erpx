import "server-only";

/** Closed-variant owner-local campaign projection for Marketing CRM analytics.
 * The enforced and legacy analytics readers share one fixed projection frame:
 * a caller-chosen projection and alias over the owner-owned campaign table and
 * null branch/department columns. No raw query text is accepted from a caller,
 * so no duplicate projection literal survives in the owner and each admitted
 * variant emits the original statement byte for byte. */
export function marketingAnalyticsCampaignSql(enforced: boolean): string {
  const projection = enforced ? "campaign.*" : "*";
  const alias = enforced ? " campaign" : "";
  return `SELECT ${projection},NULL::text branch_id,NULL::text department_id FROM marketing_campaigns${alias}`;
}
