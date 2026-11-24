import {packageClockSql} from "@erpx/server-commercial/public/purchased-capability-sql";
/** Owner-local scope CTE family. The company bind is always first; the branch
 * and department columns are parameterized so the actor and Company-only
 * variants share one byte-identical fragment. */
const scopeCte = (branch:string,department:string) => `(SELECT ?::text company_id,${branch} branch_id,${department} department_id)`;
/** Owner-local actor scope CTE shared by Recruitment read guards. The three
 * binds are company_id, branch_id and department_id in that order. */
export const talentActorScopeCte = scopeCte("?::text","?::text");
/** Owner-local Company-only actor scope CTE shared by Recruitment privacy
 * read guards. The single bind is company_id; branch_id and department_id are
 * always NULL. Byte-identical to the former inline copies. */
export const talentCompanyScopeCte = scopeCte("NULL::text","NULL::text");
/** Current-actor authority projection over the shared actor scope CTE. The
 * alias and projection are parameterized; the surrounding SQL and bind order
 * are byte-identical to the former per-guard inline copies. */
export const talentActorScopeSelect = (alias:string,projection:string) => `SELECT ${projection} FROM ${talentActorScopeCte} ${alias}`;
/** Owner-local clock CTE shared by Recruitment read guards. Byte-identical to
 * the former inline copies; the named CTE and MATERIALIZED mode are preserved. */
export const talentClockCte = (name:string) => `WITH ${name} AS MATERIALIZED (SELECT ${packageClockSql} now_ms)`;
