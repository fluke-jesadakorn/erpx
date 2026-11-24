import "server-only";

/** Closed-variant owner-local touch projection for Marketing CRM analytics.
 * The enforced and legacy analytics readers share one fixed projection frame:
 * only the row source and the company-scope join differ. The caller supplies
 * only the owner-local touch row source, never raw query text, so no duplicate
 * projection literal survives in the owner and each admitted variant emits the
 * original statement byte for byte. */
export function marketingAnalyticsTouchSql(enforced: boolean, touchRows: string): string {
  const source = enforced ? `(${touchRows})` : "marketing_touches";
  const company = enforced ? " AND lead.company_id=touch.company_id" : "";
  return `SELECT touch.*,lead.branch_id,lead.department_id FROM ${source} touch JOIN crm_leads lead ON lead.id=touch.lead_id${company}`;
}
