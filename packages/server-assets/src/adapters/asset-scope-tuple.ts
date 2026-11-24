/** Canonical owner-local fixed SQL for an Assets work-scope tuple. The private
 * attachment file authority assertion and the private attachment current-scope
 * check share this one tuple literal so the TS read tail carries no duplicate
 * statement. Callers pass only a fixed scope alias; no SQL or relation name is
 * accepted from a request. This is distinct from `assetPermissionProbeSql`,
 * which projects an assignment-read `allowed` column for purchase/metric callers. */
export const assetScopeTupleSql = (alias: string) =>
  `(SELECT ?::text company_id,?::text branch_id,?::text department_id) ${alias}`;
/** Canonical owner-local parameterized projection head. The current-assignment
 * scope probe and the fixed operational_assets projection render byte-identically
 * from this one literal; callers pass only fixed projection/FROM/tail fragments,
 * never request SQL or relation names. Locks, native plans and cross-owner
 * settlement sources are unchanged. */
export const assetProjectionFromSql = (projection:string,from:string,tail="") =>
  `SELECT ${projection} FROM${from}${tail}`;
/** Canonical owner-local current-assignment scope probe. Both the private
 * attachment file authority assertion and the private attachment current-scope
 * check share this one statement token; callers pass only a fixed projection,
 * FROM fragment and optional tail, never request SQL or relation names. */
export const assetCurrentScopeProbeSql = (projection:string,from:string,tail="") =>
  assetProjectionFromSql(projection,from,tail);
