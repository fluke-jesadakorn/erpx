import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { ppRd28PersonLegalNamesSql } from './pp-read-tail-r28-sources';
export function RequestPreviewPeoplePayrollFacts(database: SqlDatabase) {
    return { readNamedRoutePeople(companyId: string, ids: string[]) {
            return database.prepare(ppRd28PersonLegalNamesSql(ids.map(() => "?").join(","))).bind(companyId, ...ids).all<{
                user_id: string;
                legal_name: string;
            }>();
        } };
}
