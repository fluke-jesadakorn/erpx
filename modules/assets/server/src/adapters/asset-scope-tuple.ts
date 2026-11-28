/** Canonical owner-local universal head. The parameterized projection-from head,
 * the source-image witness and the work-scope tuple render byte-identically from
 * this one Rust-owned template; callers pass only fixed projection and tail
 * fragments, never request SQL or relation names. The statement token lives in
 * `asset-projection-head-sources.json`, bound into the native owner with
 * `include_str!` and exposed by `public::projection_head`; this adapter carries
 * no SQL literal. Locks, native plans and cross-owner settlement and clock
 * sources stay unchanged. */
import contract from "./asset-projection-head-sources.json" with { type: "json" };

const heads=contract as {readonly selectHead:string;readonly fromHead:string};

export const assetSelectProjectionSql = (projection:string,tail="") =>
  heads.selectHead.replace("{projection}",projection).replace("{tail}",tail);
/** Canonical owner-local fixed SQL for an Assets work-scope tuple. The private
 * attachment file authority assertion and the private attachment current-scope
 * check share this one tuple, which delegates to `assetSelectProjectionSql` so
 * the TS read tail carries no duplicate statement token. Callers pass only a
 * fixed scope alias; no SQL or relation name is accepted from a request. This is
 * distinct from `assetPermissionProbeSql`, which projects an assignment-read
 * `allowed` column for purchase/metric callers. */
export const assetScopeTupleSql = (alias: string) =>
  `(${assetSelectProjectionSql("?::text company_id,?::text branch_id,?::text department_id")}) ${alias}`;
/** Canonical owner-local parameterized projection head. The current-assignment
 * scope probe and the fixed operational_assets projection render byte-identically
 * from the universal head; callers pass only fixed projection/FROM/tail fragments,
 * never request SQL or relation names. Locks, native plans and cross-owner
 * settlement sources are unchanged. */
export const assetProjectionFromSql = (projection:string,from:string,tail="") =>
  assetSelectProjectionSql(projection,heads.fromHead.replace("{from}",from).replace("{tail}",tail));
/** Canonical owner-local current-assignment scope probe. Both the private
 * attachment file authority assertion and the private attachment current-scope
 * check share this one statement token; callers pass only a fixed projection,
 * FROM fragment and optional tail, never request SQL or relation names. */
export const assetCurrentScopeProbeSql = (projection:string,from:string,tail="") =>
  assetProjectionFromSql(projection,from,tail);
