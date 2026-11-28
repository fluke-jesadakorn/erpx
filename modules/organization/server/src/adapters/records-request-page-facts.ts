import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { activeNodeKindList } from './organization-read-tail-sources-r43';
export function RequestPageOrganizationFacts(database: SqlDatabase) {
    return { readRequestPageOptionsNodesResultcompanySettings(companyId: string, branchId: string | null, branchId2: string | null, branchId3: string | null, departmentId: string | null, departmentId2: string | null) {
            return database.prepare(activeNodeKindList("'company','branch','department'", " AND (?::text IS NULL OR id=? OR branch_id=?) AND (?::text IS NULL OR kind='branch' OR id=?) ORDER BY display_name"))
                .bind(companyId, branchId, branchId2, branchId3, departmentId, departmentId2)
                .all<{
                id: string;
                kind: string;
                display_name: string;
                branch_id: string | null;
            }>();
        } };
}
