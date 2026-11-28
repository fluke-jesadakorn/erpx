import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { nativeClearCommandAssertions } from "@erpx/collaboration-server/adapters/native-command-assertions";
export function RequestAiWorkflowFacts(database: SqlDatabase) {
    return { stageRunRequestAiEffect(authorizationKey: string, companyId: string) { return nativeClearCommandAssertions(database, companyId, authorizationKey); },
        stageRunRequestAiEffect4(command: string, companyId: string) { return nativeClearCommandAssertions(database, companyId, command); } };
}
