import "server-only";
import { nativeClearCommandAssertions } from '@erpx/collaboration-server/adapters/native-command-assertions';
import {run} from '@erpx/platform-runtime-server/public/runtime/sql';
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
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
