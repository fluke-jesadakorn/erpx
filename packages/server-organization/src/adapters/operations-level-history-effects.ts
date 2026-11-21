import "server-only";
import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import { nativeOrgStatement } from "./native-organization-owner-write";

export function operationsLevelHistoryEffects(db: SqlDatabase, companyId = '') {
    return {
        /** Native owner SQL, executed inside the caller's own command unit. */
        stageChange: (
            value: string,
            engagementId: string,
            previous: string | null,
            levelId: string,
            userId: string,
            reason: string,
            key: string,
            now: number,
        ): Promise<SqlStatement> =>
            nativeOrgStatement(
                db,
                companyId,
                'staff_level_history_insert',
                [value, engagementId, previous, levelId, userId, reason, key, now],
                key,
                now,
            ),
    };
}
