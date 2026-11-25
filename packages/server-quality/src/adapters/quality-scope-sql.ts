import "server-only";
import {packageClockSql} from "@erpx/server-commercial/public/purchased-capability-sql";
import {evidenceHead} from "./inspection-evidence-source-sql";
/** Owner-local parenthesized scope head shared by the Quality package-scope
 * CTEs. Both CTEs keep their exact former parenthesized rendering. */
export const scopeSql = (columns:string) => `(${evidenceHead(columns, '', '')})`;
/** WITH-aware parenthesized head for heads that open with a CTE rather than a
 * bare projection. No statement literal is emitted here; body is caller-owned. */
export const withScopeSql = (body:string) => `(${body})`;
/** Owner-local Company/full scope CTE shared by Quality package-obligation
 * reads. The two binds are company_id and full in that order. Byte-identical
 * to the former inline copies. */
export const qualityPackageScopeCte = scopeSql("?::text company_id,?::boolean full");
/** Owner-local clock CTE shared by Quality purchased read guards. The named
 * CTE and MATERIALIZED mode are preserved; byte-identical to the former
 * inline copies. */
export const qualityClockCte = (name:string) => `WITH ${name} AS MATERIALIZED ${scopeSql(`${packageClockSql} now_ms`)}`;
