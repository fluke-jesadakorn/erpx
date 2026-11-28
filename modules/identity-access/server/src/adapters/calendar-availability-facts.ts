import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { activeRecipientsSql } from "./iam-probe-sql";
export function calendarAvailabilityIdentityAccessFacts(db: SqlDatabase) {
    return { read1: (facts: {
            value1: string;
            value2: string[];
            value3: number;
        }) => {
            return db.prepare(activeRecipientsSql(facts.value2, "\n    ")).bind(facts.value1, ...facts.value2, facts.value3, facts.value3).all<{
                user_id: string;
            }>();
        } };
}
