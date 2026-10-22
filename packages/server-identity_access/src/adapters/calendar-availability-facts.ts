import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function calendarAvailabilityIdentityAccessFacts(db: SqlDatabase) {
    return { read1: (facts: {
            value1: string;
            value2: string[];
            value3: number;
        }) => {
            return db.prepare(`SELECT DISTINCT user_id FROM assignments WHERE company_id=? AND user_id IN (${facts.value2.map(() => "?").join(",")})
    AND revoked_at IS NULL AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?)`).bind(facts.value1, ...facts.value2, facts.value3, facts.value3).all<{
                user_id: string;
            }>();
        } };
}
