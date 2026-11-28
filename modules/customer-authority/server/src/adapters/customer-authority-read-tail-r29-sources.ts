import 'server-only';

/** One parameterized review-row projection shared by the identity-change and
 *  duplicate-review workspace readers. Only closed aliases, table/column text,
 *  fixed joins and the caller's manager-eligibility predicate are supplied; every
 *  rendered statement keeps its original SQL bytes. Control-plane locks, writes
 *  and the integrated native read kinds are not part of this read-only family;
 *  the TS consumers remain intact. */
export const reviewEligibilityRows = (
  alias: string,
  table: string,
  columns: string,
  joins: string,
  manager: string,
) =>
  `SELECT ${alias}.*,${columns},CASE WHEN ${manager} THEN 1 ELSE 0 END actor_eligible FROM ${table} ${alias} ${joins} WHERE ${alias}.company_id=? ORDER BY ${alias}.updated_at DESC,${alias}.id`;
