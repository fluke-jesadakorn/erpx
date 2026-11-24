/** Shared query heads for the r33 identity_access read-tail families. Each
 * helper pins the fixed projection and leading predicates; callers inject only
 * their varying tail (including original whitespace and newlines) so every
 * rendered statement stays byte-identical to its pre-dedupe literal. No lock,
 * write, session authority, native kind or earlier-round helper is changed. */
export const assignmentAuthorityProbeSql = (tail: string): string =>
  `SELECT 1 FROM assignments a WHERE a.id=? AND a.company_id=? AND a.user_id=? AND ${tail}`;

/** Distinct active Company assignment users; the caller's tail carries the
 * remaining predicates and every ? bind stays at the call site. */
export const assignmentCompanyUsersSql = (tail: string): string =>
  `SELECT DISTINCT a.user_id FROM assignments a WHERE a.company_id=? AND ${tail}`;

/** Active work-context probe; the caller's tail carries the remaining
 * predicates, including original newline tails and revokedSessionProbeSql
 * composition. Row locks stay at the call site. */
export const activeWorkContextProbeSql = (tail: string): string =>
  `SELECT 1 FROM active_work_contexts WHERE session_id=? AND company_id=? AND user_id=?${tail}`;
