import {packageClockSql} from "@erpx/server-commercial/public/purchased-capability-sql";
/** Owner-local actor scope CTE shared by Recruitment read guards. The three
 * binds are company_id, branch_id and department_id in that order. */
export const talentActorScopeCte = "(SELECT ?::text company_id,?::text branch_id,?::text department_id)";
/** Owner-local Company-only actor scope CTE shared by Recruitment privacy
 * read guards. The single bind is company_id; branch_id and department_id are
 * always NULL. Byte-identical to the former inline copies. */
export const talentCompanyScopeCte = "(SELECT ?::text company_id,NULL::text branch_id,NULL::text department_id)";
/** Owner-local clock CTE shared by Recruitment read guards. Byte-identical to
 * the former inline copies; the named CTE and MATERIALIZED mode are preserved. */
export const talentClockCte = (name:string) => `WITH ${name} AS MATERIALIZED (SELECT ${packageClockSql} now_ms)`;
