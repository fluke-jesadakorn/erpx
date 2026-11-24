/** Procurement read-tail r24 shared fragments. Read-only text only: each builder
 * renders the exact original statement so binds, ordering, locks and writes are
 * unchanged. The r17/r18/r19/r20/r21/r22/r23 helpers stay untouched. */
/** Current-member self exclusion used by the work eligibility counts. The owner
 * alias, company/user expressions and owner-user predicate are the only degrees
 * of freedom; no binds, locks or writes are introduced. */
export const memberScopeExclusion = (
  members: string,
  owner: string,
  company: string,
  user: string,
  ownerUserPredicate: string,
) =>
  `NOT EXISTS(SELECT 1 FROM (${members}) cm JOIN (${members}) ${owner} ON ${owner}.company_id=cm.company_id AND ${owner}.person_id=cm.person_id WHERE cm.company_id=${company} AND cm.user_id=${user} AND ${owner}.user_id${ownerUserPredicate})`;
/** Count of purchase-capable current actors in the record's branch/department
 * scope. Renders the exact original predicate; `tail` carries each caller's
 * read-only extra conditions verbatim. */
export const eligiblePurchaseCount = (
  assignments: string,
  alias: string,
  company: string,
  branch: string,
  department: string,
  tail = "",
) =>
  `(SELECT count(DISTINCT ${alias}.user_id) FROM (${assignments}) ${alias} WHERE ${alias}.company_id=${company} AND iam_assignment_active(${alias}.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(${alias}.role,${alias}.company_id,'commerce.purchase') AND (${alias}.scope_kind='holding' OR (${alias}.scope_kind='branch' AND ${alias}.scope_node_id=${branch}) OR (${alias}.scope_kind='department' AND ${alias}.scope_node_id=${department}))${tail})`;
