import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
export function calendarAppointmentsIdentityAccessFacts(db: SqlDatabase) {
    return { read9: (facts: {
            value1: string[];
        }) => {
            return db.prepare(`SELECT id,name FROM "user" WHERE id IN (${facts.value1.map(() => "?").join(",")})`).bind(...facts.value1).all<{
                id: string;
                name: string;
            }>();
        } };
}
