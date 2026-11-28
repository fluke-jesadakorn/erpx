/** Shared query heads for the r35 identity_access read-tail families. Each
 * helper owns one statement token and pins the fixed projection/table/predicates;
 * a closed-union discriminator selects the exact pre-dedupe head so rendered SQL
 * stays byte-identical and bind order stays fixed. No lock, write, session
 * authority, native kind or earlier-round helper changes. */

/** Scope-branch ancestor probe nested in the four authority guards. The closed
 * union selects the caller's company alias and the original whitespace shape. */
export type ScopeBranchShape = 'a' | 'a-multiline' | 'grant' | 'g';
export const scopeBranchExistsSql = (from: string, shape: ScopeBranchShape): string => {
  const company = shape === 'grant' ? 'grant_row.company_id' : shape === 'g' ? 'g.company_id' : 'a.company_id';
  const gap = shape === 'a-multiline' ? '\n                    ' : ' ';
  return `SELECT 1 ${from}${gap}WHERE b.id=n.branch_id AND b.company_id=${company} AND b.status='active'`;
};

/** Fixed HR appointment seat head shared by the two stored-authority probes;
 * each caller keeps its own already-authorized WHERE tail and bind list. */
export const hrSeatExistsSql = (seatsFrom: string, peopleJoin: string, engagementJoin: string): string =>
  `SELECT 1 ${seatsFrom} ${peopleJoin} ON p.id=s.person_id AND p.company_id=s.company_id AND p.status='active' AND p.merged_into_id IS NULL ${engagementJoin} ON e.person_id=p.id AND e.company_id=p.company_id AND e.status='active'`;

/** Boolean role probe used by the IAM command and transaction services. */
export const iamRoleAllowsAliasSql = (alias: 'allowed' | 'ok'): string =>
  `SELECT iam_role_allows($1,$2,$3) ${alias}`;
