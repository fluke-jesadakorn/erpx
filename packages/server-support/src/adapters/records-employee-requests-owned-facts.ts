import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { nativeSupportReadAll, SUPPORT_RD_SERVICE_TEAM_NAME } from './native-support-read';

/** Owner-native read: the statement is planned by the Support native read
 * port (`RD_SERVICE_TEAM_NAME`) and executed on the caller's open unit.
 * This module carries no SQL for this read. */
export function EmployeeRequestsSupportOwnedFacts(database: SqlDatabase) {
    return { readEmployeeRequestDetailTeam(team_id: string, companyId: string) {
            return nativeSupportReadAll<{ name: string }>(
                SUPPORT_RD_SERVICE_TEAM_NAME,
                [team_id, companyId],
                database,
            ).then(result => result.results[0] ?? null);
        } };
}
