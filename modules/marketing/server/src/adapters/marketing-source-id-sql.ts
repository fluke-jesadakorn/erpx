import 'server-only';
import {marketingPurchaseUnionSql} from './marketing-purchase-union-sql';

/** Owner-local fixed source-id projection for Marketing purchase-scope reads.
 * The four source tables share one UNION ALL frame; a single admitted builder
 * emits the byte-identical statement so no duplicate projection literal
 * survives in the owner. Callers never supply raw query text. */
const sourceIdTables = ['marketing_assets','marketing_lead_candidates','marketing_campaigns','marketing_campaign_costs'] as const;

export const marketingSourceIdsSql = (): string =>
  marketingPurchaseUnionSql(sourceIdTables.map(table => ['id', table] as const));
