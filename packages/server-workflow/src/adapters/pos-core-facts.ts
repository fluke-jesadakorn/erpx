import "server-only";
import { nativeClearCommandAssertions } from '@erpx/server-collaboration/adapters/native-command-assertions';
import {run} from '@erpx/server-runtime/public/runtime/sql';
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function posCoreWorkflowFacts(db: SqlDatabase) {
    return {
        writeCommandAssertions1: (facts: {
            key: string;
            companyId: string;
        }) => nativeClearCommandAssertions(db, facts.companyId, facts.key).then(s=>s.run()),
        writeCommandAssertions2: (facts: {
            key: string;
            companyId: string;
        }) => nativeClearCommandAssertions(db, facts.companyId, facts.key).then(s=>s.run())
    };
}
