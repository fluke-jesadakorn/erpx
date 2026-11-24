/** One parameterized beta_policy point-read family shared by owner callers.
 * Only a fixed projection token is admitted; the table and predicate stay here
 * so the policy, member-capacity and branch-capacity readers cannot drift. */
export type BetaPolicyPointProjection = '*' | 'member_limit' | 'branch_limit';
export const betaPolicyPointSql = (projection: BetaPolicyPointProjection) =>
  `SELECT ${projection} FROM beta_policy WHERE company_id=?`;
