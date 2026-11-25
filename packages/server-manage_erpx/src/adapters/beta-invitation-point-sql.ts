/** Byte-identical beta_team_invitations point-read family shared by the beta team fact unit.
 * Callers pass only trusted literals: a fixed projection and key column.
 * Every caller keeps its own binds and statement; no request text is interpolated. */
import {betaPointByKeySql} from './beta-point-by-key-sql';
export type BetaInvitationPointProjection = '*' | 'accepted_user_id';
export type BetaInvitationPointKey = 'token_hash' | 'id';
export const betaInvitationPointSql = (
  projection: BetaInvitationPointProjection,
  key: BetaInvitationPointKey,
) => betaPointByKeySql('team_invitations', projection, key);
