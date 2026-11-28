import 'server-only';
import contract from "./identity-snapshot-sources.json" with { type: "json" };

/** The single fixed beta_policy owner projection shared by the IAM statement
 * snapshot and the hospitality/Files/Runtime native joins. The exact bytes
 * live in the Rust-owned contract `identity-snapshot-sources.json`, bound into
 * Rust with `include_str!`; both public surfaces reuse this one declaration so
 * their query text cannot drift. */
export const betaPolicyProjection =
  (contract as { readonly betaPolicyProjection: string }).betaPolicyProjection;

/** Closed explicit-column owner projections for the supplied IAM statement snapshot.
 * No caller-supplied table, SQL, predicate or alias is accepted. */
export function identitySnapshotSources(){return {
  "beta_access_beta_policy_from": `FROM (${betaPolicyProjection}) beta_policy`,
  "iam_api_beta_team_invitations_i_from":
    (contract as { readonly iamApiBetaTeamInvitationsFrom: string }).iamApiBetaTeamInvitationsFrom,
} as const;}
