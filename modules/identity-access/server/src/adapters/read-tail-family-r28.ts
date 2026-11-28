/** Shared query heads for the r28 identity_access read-tail families. Each
 * helper owns a single statement token and pins the projection, table and fixed
 * predicates; callers inject only the varying fragment. Rendered statements are
 * byte-identical to the pre-dedupe literals. No lock, write, session authority,
 * native kind or earlier-round helper is changed. */
export const userIdNameEmailInSql = (rest: string): string =>
  `SELECT id,name,email FROM "user" WHERE id IN (${rest}`;

export const iamReviewProbeSql = (predicate: string): string =>
  `SELECT 1 FROM iam_reviews WHERE ${predicate}`;

export const iamGroupProbeSql = (tail: string): string =>
  `SELECT 1 FROM iam_groups ${tail}`;

export const iamMfaProbeSql = (tail: string): string =>
  `SELECT 1 FROM iam_mfa ${tail}`;

export const peopleDistinctNameSql = (tail: string): string =>
  `SELECT DISTINCT a.user_id id,coalesce(p.legal_name,a.user_id) name,${tail}`;

export const assignmentVersionSql = (predicate: string): string =>
  `SELECT version FROM assignments WHERE id=? AND user_id=? AND ${predicate}`;

export const assignmentRoleSql = (predicate: string): string =>
  `SELECT role FROM assignments WHERE id=? AND user_id=? AND company_id=? AND ${predicate}`;
