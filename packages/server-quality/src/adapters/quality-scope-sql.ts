import "server-only";
import {packageClockSql} from "@erpx/server-commercial/public/purchased-capability-sql";
/** Owner-local parenthesized scope head shared by the Quality package-scope
 * CTEs. Both CTEs keep their exact former parenthesized rendering. */
const scopeSql = (columns:string) => `(SELECT ${columns})`;
/** Owner-local Company/full scope CTE shared by Quality package-obligation
 * reads. The two binds are company_id and full in that order. Byte-identical
 * to the former inline copies. */
export const qualityPackageScopeCte = scopeSql("?::text company_id,?::boolean full");
/** Owner-local clock CTE shared by Quality purchased read guards. The named
 * CTE and MATERIALIZED mode are preserved; byte-identical to the former
 * inline copies. */
export const qualityClockCte = (name:string) => `WITH ${name} AS MATERIALIZED ${scopeSql(`${packageClockSql} now_ms`)}`;
