import 'server-only';

/** Owner-local company-scoped list head. The two admitted Marketing list
 * reads (assets and pending lead candidates) share one list head; callers
 * pass only the table and their exact scope tail, so emitted bytes stay fixed. */
export const marketingCompanyListSql = (
  table: 'marketing_assets' | 'marketing_lead_candidates',
  scope: string,
): string => `SELECT * FROM ${table} WHERE company_id=?${scope} ORDER BY updated_at DESC`;
