/** One parameterized beta_bootstrap point-read family shared by the gateway
 * scope probe and the singleton binding lookup. Only fixed projections, a
 * fixed alias and an optional trailing predicate are admitted; callers keep
 * their own binds and statements, and no request text is interpolated. */
export type BetaBootstrapProjection = '1' | 'holding_id';
export type BetaBootstrapAlias = '' | 'h';
export const betaBootstrapPointSql = (
  projection: BetaBootstrapProjection,
  alias: BetaBootstrapAlias = '',
  tail = '',
) =>
  `SELECT ${projection} FROM beta_bootstrap${alias ? ` ${alias}` : ''} WHERE ${alias ? `${alias}.` : ''}singleton=1${tail}`;
