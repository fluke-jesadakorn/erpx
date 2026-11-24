import "server-only";
/** Canonical owner-local IAM scope guard shared by the Work owner-eligibility
 * and can-review images and the Work/Plan purchase-source eligibility witness.
 * The assignment/principal/role/status tail lives here once; callers pass only
 * owner-fixed FROM/JOIN, aliases, row alias, user, a closed permission and a
 * scope fragment. Each composed string stays byte-identical to its pre-dedupe
 * literal; no LOCK/UPDATE statement is accepted or changed. */
export const iamScopeGuardSql = (
  from: string,
  a: string,
  p: string,
  e: string,
  row: string,
  user: string,
  perm: "records.write" | "records.review",
  scope: string,
): string =>
  `EXISTS(SELECT 1 FROM ${from} WHERE ${a}.company_id=${row}.company_id AND ${a}.user_id=${user} AND iam_assignment_active(${a}.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(${a}.user_id,${a}.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(${a}.role,${a}.company_id,'${perm}') AND ${scope} AND ${p}.status='active' AND ${p}.merged_into_id IS NULL AND ${e}.status='active' AND ${e}.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (${e}.end_date IS NULL OR ${e}.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))`;
