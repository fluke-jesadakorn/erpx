import "server-only";
import { nativeClearCommandAssertions } from '@erpx/server-collaboration/adapters/native-command-assertions';
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
const run = async (db: SqlDatabase, sql: string, ...values: unknown[]) => db
    .prepare(sql)
    .bind(...values)
    .run();
export function pmsWorkflowFacts(db: SqlDatabase) {
    return {
        deleteCommandAssertions: (facts: {
            id: `${string}-${string}-${string}-${string}-${string}`;
            companyId: string;
        }) => nativeClearCommandAssertions(db, facts.companyId, facts.id).then(s=>s.run())
    };
}
