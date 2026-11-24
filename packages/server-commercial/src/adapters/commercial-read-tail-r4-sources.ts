import "server-only";
/** Commercial read-tail r4 shared read-only SQL fragments. Each builder renders
 * the exact original literal so binds, ordering and lock/write behavior stay
 * unchanged; every consumer still prepares and binds its own independent
 * statement and keeps its exports. The r1/r3 helpers stay untouched. Pure read:
 * no writes, no LOCK TABLE, no FOR UPDATE/SHARE and no advisory lock. */

/** Purchase-fact aggregate used by readCompanyPurchaseFacts. */
export const purchaseFactAgg = (source: string, alias: string, order: string) =>
  `SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${order}) FROM ${source} ${alias} WHERE company_id=?`;

/** Coverage singleton probe used by the usage/seat repositories. */
export const coverageProbe = (column: string, source: string, singleton: string) =>
  `SELECT ${column} FROM ${source} WHERE singleton=${singleton}`;
