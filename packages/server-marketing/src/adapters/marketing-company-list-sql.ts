import 'server-only';
import { marketingReadTailSql } from './read-tail-sql';

/** Owner-local company-scoped list head. The two admitted Marketing list
 * reads (assets and pending lead candidates) share one list head; callers
 * pass only the table and their exact scope tail, so emitted bytes stay fixed.
 * r45: the head renders through the owner-local read-tail renderer. */
export const marketingCompanyListSql = (
  table: 'marketing_assets' | 'marketing_lead_candidates',
  scope: string,
): string => marketingReadTailSql(table, ` WHERE company_id=?${scope} ORDER BY updated_at DESC`);
