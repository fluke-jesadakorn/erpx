import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import type { WorkContext } from "../../identity_access/public/authorization";
const query = async <T>(db: SqlDatabase, sql: string, ...values: unknown[]) => (await db
    .prepare(sql)
    .bind(...values)
    .all<T>()).results;
export function pmsOrganizationFacts(db: SqlDatabase) {
    return {
        readOrganizationNodes: (facts: {
            companyId: string;
            fact1: readonly string[];
            actor: WorkContext;
        }) => query<{
            id: string;
            name: string;
        }>(db, `SELECT id,display_name name FROM organization_nodes WHERE company_id=? AND kind='branch' AND status='active'${facts.actor.branchId ? " AND id=?" : ""}`, facts.companyId, ...facts.fact1),
        readOrganizationNodes2: (facts: {
            branchId: string;
            companyId: string;
        }) => db.prepare("SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active' FOR SHARE").bind(facts.branchId, facts.companyId).first()
    };
}
