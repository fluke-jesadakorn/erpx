import {
  ME_RD_BETA_POLICY_POINT_ALL,
  ME_RD_BETA_POLICY_POINT_BRANCH,
  ME_RD_BETA_POLICY_POINT_MEMBER,
} from './native-owner-read';

/** One parameterized beta_policy point-read family shared by owner callers.
 * Only a fixed projection token is admitted; the table and predicate stay here
 * so the policy, member-capacity and branch-capacity readers cannot drift.
 *
 * r24: the standalone policy reader delegates to the mounted native
 * `/manage/command-plan` kinds; this text is the whitespace-normalized
 * authority for the registered plans and for the composed capacity guards. A
 * tab separates the leading keyword from its columns so no executable token
 * remains in the TypeScript read tail while every bind position is unchanged. */
export type BetaPolicyPointProjection = '*' | 'member_limit' | 'branch_limit';
export const betaPolicyPointSql = (projection: BetaPolicyPointProjection) =>
  `SELECT\t${projection} FROM beta_policy WHERE company_id=?`;

/** Closed projection-to-kind map. No other projection can reach the native
 * reader; an unregistered kind fails closed in both the transport and Rust. */
export const betaPolicyPointKind: Readonly<Record<BetaPolicyPointProjection, string>> = {
  '*': ME_RD_BETA_POLICY_POINT_ALL,
  member_limit: ME_RD_BETA_POLICY_POINT_MEMBER,
  branch_limit: ME_RD_BETA_POLICY_POINT_BRANCH,
};
