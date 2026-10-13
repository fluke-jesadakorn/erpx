import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
export async function accessUnitFacts(database: SqlDatabase, companyId: string) {
  return (await database.prepare(`SELECT id,display_name AS name,kind,branch_id AS "branchId" FROM organization_nodes
    WHERE company_id=? AND status='active' AND kind IN ('branch','department') ORDER BY display_name,id`).bind(companyId)
    .all<{id:string;name:string;kind:string;branchId:string|null}>()).results;
}
