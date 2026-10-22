import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { activeStaffSelect } from "@erpx/server-operations/adapters/staff-policy";
export function calendarCommandsOperationsFacts(db: SqlDatabase) {
    return { read2: (facts: {
            value1: string;
            value2: number;
            value3: string;
        }) => db.prepare(`${activeStaffSelect} AND p.id=?`).bind(facts.value1, facts.value2, facts.value2, facts.value3).all<{
            person_id: string;
            user_id: string;
            department_id: string | null;
            branch_id: string | null;
        }>().then(result => result.results) };
}
