/** One byte-identical beta_* point-read head shared by the invitation and slot
 * key lookups. Only closed table, projection and key tokens are admitted; the
 * caller keeps its own optional trailing predicate, binds and statement, so
 * rendered SQL cannot drift and no request text is interpolated. */
export type BetaPointByKeyTable = 'team_invitations' | 'slots';
export type BetaPointByKeyProjection =
  | '*'
  | 'accepted_user_id'
  | '1'
  | 'slug,company_name,branch_name,state';
export type BetaPointByKeyColumn = 'token_hash' | 'id' | 'holding_id' | 'owner_user_id';
export const betaPointByKeySql = (
  table: BetaPointByKeyTable,
  projection: BetaPointByKeyProjection,
  key: BetaPointByKeyColumn,
  tail = '',
) => `SELECT ${projection} FROM beta_${table} WHERE ${key}=?${tail}`;
