/** Shared query heads for the r34 identity_access read-tail families. Each
 * helper owns one statement token and pins the fixed projection/table/predicates;
 * a closed-union discriminator selects the exact pre-dedupe head so every
 * rendered statement stays byte-identical and every bind stays in place. No
 * lock, write, session authority, native kind or earlier-round helper changes. */

/** Scope-node existence probe used by both workflow-role user queries. */
export const workflowRoleNodeProbeSql = (
  from: string,
  kind: 'branch' | 'department',
): string => `SELECT 1 ${from} WHERE id=? AND company_id=staff.company_id AND kind='${kind}' AND status='active'`;

/** Node-id subquery used by the IAM scope-filtered reads. The closed union keeps
 * the original WHERE order (and therefore the original bind order) intact. */
export const iamServiceNodeSubquerySql = (
  from: string,
  order: 'branch-first' | 'company-first',
): string => `SELECT id ${from} WHERE ${order === 'branch-first' ? 'branch_id=? AND company_id=?' : 'company_id=? AND branch_id=?'}`;

/** Exact pre-dedupe head shape of each iam_members identity-version probe. Only
 * the server-owned company/user expressions for the interpolated shapes are
 * passed in; all predicate text is selected from this closed union. */
export type IamMemberVersionProbeShape = 'cast' | 'plain' | 'grant' | 'interp' | 'report';
export const iamMemberVersionProbeSql = (
  shape: IamMemberVersionProbeShape,
  company: string = '',
  user: string = '',
): string => {
  const aliased = shape === 'interp' || shape === 'report';
  const column = shape === 'report' ? 'm.version' : 'version';
  const table = aliased ? 'iam_members m' : 'iam_members';
  const companyExpr = shape === 'cast' ? '?::text'
    : shape === 'grant' ? 'grant_row.company_id'
    : shape === 'report' ? 'g.company_id'
    : shape === 'interp' ? company
    : '?';
  const userExpr = shape === 'cast' ? '?::text'
    : shape === 'grant' ? 'grant_row.user_id'
    : shape === 'report' ? 'g.user_id'
    : shape === 'interp' ? user
    : '?';
  return `SELECT ${column} FROM ${table} WHERE ${aliased ? 'm.company_id' : 'company_id'}=${companyExpr} AND ${aliased ? 'm.user_id' : 'user_id'}=${userExpr}`;
};
