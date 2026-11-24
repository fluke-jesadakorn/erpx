import 'server-only';

/** Owner-local four-arm purchase source union. Callers pass the admitted
 * [projection, from] arms; one builder emits the exact ` UNION ALL ` frame so
 * no duplicate arm literal survives in the owner. */
export const marketingPurchaseUnionSql = (
  arms: readonly (readonly [string, string])[],
): string => arms.map(([projection, from]) => `SELECT ${projection} FROM ${from}`).join(' UNION ALL ');
