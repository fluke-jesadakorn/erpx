import "server-only";
import { run } from "../../pos/public/pos-core";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
export function posCoreWorkflowFacts(db: SqlDatabase) {
    return {
        writeCommandAssertions1: (facts: {
            key: string;
        }) => run(db, 'DELETE FROM command_assertions WHERE command_id=?', facts.key),
        writeCommandAssertions2: (facts: {
            key: string;
        }) => run(db, 'DELETE FROM command_assertions WHERE command_id=?', facts.key)
    };
}
