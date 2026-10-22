import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { socialPersonOrganizationProjection } from '@erpx/server-organization/public/collaboration-home-projections';
export function collaborationHomeSources(db: SqlDatabase) {
    return { guardPersonAudience(key: string, value: string, userId: string, companyId: string, now: number, now2: number, departmentId: string | null, departmentId2: string | null, branchId: string | null, branchId2: string | null, branchId3: string | null) {
            return assertion(db, key, value, `EXISTS(SELECT 1 FROM assignments a WHERE a.user_id=? AND a.company_id=? AND a.revoked_at IS NULL
      AND a.valid_from<=? AND a.valid_until>? AND (CASE
        WHEN ?::text IS NOT NULL THEN a.scope_kind='department' AND a.scope_node_id=?
        WHEN ?::text IS NOT NULL THEN a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=?)
          OR (a.scope_kind='department' AND ${socialPersonOrganizationProjection()})
        ELSE TRUE END))`, [userId, companyId, now, now2, departmentId, departmentId2,
                branchId, branchId2, branchId3]);
        } };
}
