/** Byte-identical beta_team_invitations point-read family shared by the beta team fact unit.
 * Callers pass only trusted literals: a fixed projection and key column.
 * Every caller keeps its own binds and statement; no request text is interpolated. */
export type BetaInvitationPointProjection = '*' | 'accepted_user_id';
export type BetaInvitationPointKey = 'token_hash' | 'id';
export const betaInvitationPointSql = (
  projection: BetaInvitationPointProjection,
  key: BetaInvitationPointKey,
) => `SELECT ${projection} FROM beta_team_invitations WHERE ${key}=?`;
