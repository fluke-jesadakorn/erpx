/** Shared query heads for the r32 identity_access read-tail families. Each
 * helper pins the fixed table, projection and leading predicates; callers inject
 * only their varying tail (including original whitespace and newlines) so every
 * rendered statement stays byte-identical to its pre-dedupe literal. No lock,
 * write, session authority, native kind or earlier-round helper is changed. */
export const assignmentCompanyUserProbeSql = (tail: string): string =>
  `SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND ${tail}`;

/** pg positional-parameter pair probe; the caller's tail carries the fixed
 * protected/emergency predicate and the $n binds stay at the call site. */
export const iamMembersPairSql = (tail: string): string =>
  `SELECT 1 FROM iam_members WHERE company_id=$1 AND user_id=$2 AND ${tail}`;
