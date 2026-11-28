import 'server-only';
import { marketingAnalyticsRowsSql } from './analytics-null-scope-rows';
/** Owner-local fixed projection for Marketing campaign-cost analytics rows.
 * The enforced report definition and the legacy fallback share this one
 * admitted projection through the shared NULL-scope head; callers supply only
 * the owner-owned source fragment. */
export const marketingAnalyticsCostRows = (source: string): string =>
  marketingAnalyticsRowsSql('cost.*', source);
