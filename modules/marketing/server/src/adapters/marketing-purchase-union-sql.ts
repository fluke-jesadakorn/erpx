import 'server-only';
import { marketingSelectHeadSql } from './select-head-sql';

/** Owner-local four-arm purchase source union. Callers pass the admitted
 * [projection, from] arms; each arm renders its exact statement on the shared
 * selection head and this builder keeps the ` UNION ALL ` join in the caller,
 * so no duplicate arm literal survives in the owner. */
export const marketingPurchaseUnionSql = (
  arms: readonly (readonly [string, string])[],
): string => arms.map(([projection, from]) => marketingSelectHeadSql(projection, ` FROM ${from}`)).join(' UNION ALL ');
