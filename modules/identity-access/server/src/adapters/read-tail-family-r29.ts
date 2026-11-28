/** Shared query heads for the r29 identity_access assignment probe families.
 * Each helper pins one statement token and the fixed table/projection predicates;
 * callers inject only their varying fragment (including original whitespace and
 * newlines) so every rendered statement stays byte-identical to its pre-dedupe
 * literal. No lock, write, session authority, native kind or earlier-round
 * helper is changed. */
export const assignmentActiveProbeSql = (tail: string): string =>
  `SELECT 1 FROM assignments WHERE id=? AND user_id=? AND ${tail}`;

export const assignmentUserScopeProbeSql = (tail: string): string =>
  `SELECT 1 FROM assignments WHERE user_id=? AND company_id=? AND ${tail}`;

export const distinctAssigneeSql = (tail: string): string =>
  `SELECT DISTINCT user_id FROM assignments WHERE company_id=? AND ${tail}`;
