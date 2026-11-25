import "server-only";
import {settlementRefSource} from "./asset-settlement-refs";
/** Canonical owner-local fixed SQL for the Maintenance attachment target
 * descriptor read. The operational (module-scoped) and policy (permission
 * checked) fact sources share this one target-lookup statement: both project
 * company_id, branch_id, department_id and version for an id + company_id
 * predicate. r29 delegates the descriptor head to the owner-local
 * settlementRefSource so no second row-projection head is carried in the TS
 * read tail; the composed SQL and caller binds stay byte-identical. Callers
 * pass only owner-owned, fixed projection fragments and an optional closed
 * extra predicate, never request-authored SQL. */
export const attachmentDescriptorReadSql = (
  table: string,
  branch: string,
  department: string,
  version: string,
  extra = "",
): string =>
  settlementRefSource(`company_id,${branch},${department},${version}`,table,` WHERE id=? AND company_id=?${extra ? ` AND ${extra}` : ""}`);
