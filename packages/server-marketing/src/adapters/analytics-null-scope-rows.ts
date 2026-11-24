import "server-only";

/** Owner-local fixed NULL-scope analytics row frame (r39). The campaign
 * projection (enforced and legacy) and the campaign-cost projection share one
 * byte-identical renderer: a caller picks the admitted projection and the
 * owner-owned source, and each admitted pair emits its original statement byte
 * for byte. No caller-authored query text crosses this boundary. FOR SHARE
 * locks, settlement command_receipts and native read ports stay untouched. */
export function marketingAnalyticsRowsSql(projection: string, source: string): string {
  return `SELECT ${projection},NULL::text branch_id,NULL::text department_id FROM ${source}`;
}
