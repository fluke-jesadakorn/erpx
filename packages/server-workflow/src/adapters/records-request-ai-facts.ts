import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { nativeClearCommandAssertions } from "@erpx/server-collaboration/adapters/native-command-assertions";
export function RequestAiWorkflowFacts(database: SqlDatabase) {
    return { stageRunRequestAiEffect(authorizationKey: string, companyId: string) { return nativeClearCommandAssertions(database, companyId, authorizationKey); },
        stageRunRequestAiEffect4(command: string, companyId: string) { return nativeClearCommandAssertions(database, companyId, command); } };
}
