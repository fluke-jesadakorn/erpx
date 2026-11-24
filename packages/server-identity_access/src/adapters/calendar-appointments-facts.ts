import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { idNameByUserIdsSql } from "./iam-probe-sql";
export function calendarAppointmentsIdentityAccessFacts(db: SqlDatabase) {
    return { read9: (facts: {
            value1: string[];
        }) => {
            return db.prepare(idNameByUserIdsSql(facts.value1)).bind(...facts.value1).all<{
                id: string;
                name: string;
            }>();
        } };
}
