import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {ORG_ACTIVE_BRANCHES_SOURCE} from './org-sources';
import {nodeActiveScope} from './organization-column-alias-families';
export function collaborationHomeSources(db: SqlDatabase) {
    return { async branches(companyId: string, branchId: string | null, branchId2: string | null) {
            return db.prepare(ORG_ACTIVE_BRANCHES_SOURCE)
                .bind(companyId, branchId, branchId2).all<{
                id: string;
                display_name: string;
            }>();
        }, guardOrganizationAudience(key: string, value: "audience", id: string, companyId: string, kind: "branch" | "department") {
            return assertion(db, key, value, `EXISTS(${nodeActiveScope('kind')})`, [id, companyId, kind]);
        } };
}
