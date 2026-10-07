import "server-only";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
export function RequestPreviewPeoplePayrollFacts(database: SqlDatabase) {
    return { readNamedRoutePeople(companyId: string, ids: string[]) {
            return database.prepare(`SELECT user_id,legal_name FROM hr_people WHERE company_id=? AND merged_into_id IS NULL AND user_id IN (${ids.map(() => "?").join(",")})`).bind(companyId, ...ids).all<{
                user_id: string;
                legal_name: string;
            }>();
        } };
}
