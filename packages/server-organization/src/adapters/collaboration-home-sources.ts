import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
export function collaborationHomeSources(db: SqlDatabase) {
    return { async branches(companyId: string, branchId: string | null, branchId2: string | null) {
            return db.prepare("SELECT id,display_name FROM organization_nodes WHERE kind='branch' AND company_id=? AND status='active' AND (?::text IS NULL OR id=?)")
                .bind(companyId, branchId, branchId2).all<{
                id: string;
                display_name: string;
            }>();
        }, guardOrganizationAudience(key: string, value: "audience", id: string, companyId: string, kind: "branch" | "department") {
            return assertion(db, key, value, "EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind=? AND status='active')", [id, companyId, kind]);
        } };
}
