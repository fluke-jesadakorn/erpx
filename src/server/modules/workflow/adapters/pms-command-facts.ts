import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
const run = async (db: SqlDatabase, sql: string, ...values: unknown[]) => db
    .prepare(sql)
    .bind(...values)
    .run();
export function pmsWorkflowFacts(db: SqlDatabase) {
    return {
        deleteCommandAssertions: (facts: {
            id: `${string}-${string}-${string}-${string}-${string}`;
        }) => run(db, "DELETE FROM command_assertions WHERE command_id=?", facts.id)
    };
}
