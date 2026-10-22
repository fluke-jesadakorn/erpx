import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function pmsRolloutOrganizationFacts(db: SqlDatabase) {
    return {
        organization: (companyId: string) => db.prepare("SELECT 1 FROM organization_nodes WHERE company_id=? AND kind='branch' AND status='active' LIMIT 1").bind(companyId).first()
    };
}
