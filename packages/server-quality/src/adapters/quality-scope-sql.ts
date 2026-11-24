import "server-only";
import {packageClockSql} from "@erpx/server-commercial/public/purchased-capability-sql";
/** Owner-local Company/full scope CTE shared by Quality package-obligation
 * reads. The two binds are company_id and full in that order. Byte-identical
 * to the former inline copies. */
export const qualityPackageScopeCte = "(SELECT ?::text company_id,?::boolean full)";
/** Owner-local clock CTE shared by Quality purchased read guards. The named
 * CTE and MATERIALIZED mode are preserved; byte-identical to the former
 * inline copies. */
export const qualityClockCte = (name:string) => `WITH ${name} AS MATERIALIZED (SELECT ${packageClockSql} now_ms)`;
