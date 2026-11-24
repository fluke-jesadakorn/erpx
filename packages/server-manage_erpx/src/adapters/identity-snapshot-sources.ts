import 'server-only';
/** The single fixed beta_policy owner projection shared by the IAM statement
 * snapshot and the hospitality/Files/Runtime native joins. Both public
 * surfaces reuse this one declaration so their query text cannot drift. */
export const betaPolicyProjection = 'SELECT "company_id", "member_limit", "branch_limit", "ai_monthly_limit", "storage_bytes", "created_at" FROM beta_policy' as const;
/** Closed explicit-column owner projections for the supplied IAM statement snapshot.
 * No caller-supplied table, SQL, predicate or alias is accepted. */
export function identitySnapshotSources(){return {
  "beta_access_beta_policy_from": `FROM (${betaPolicyProjection}) beta_policy`,
  "iam_api_beta_team_invitations_i_from": "FROM (SELECT \"id\", \"email\", \"branch_id\", \"invited_by\", \"token_hash\", \"expires_at\", \"accepted_user_id\", \"accepted_at\", \"revoked_at\", \"created_at\", \"role\" FROM beta_team_invitations) i"
} as const;}
