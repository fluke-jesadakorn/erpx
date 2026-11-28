import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { nativeClearCommandAssertions } from "@erpx/collaboration-server/adapters/native-command-assertions";
export function GuidedIntakeWorkflowFacts(database: SqlDatabase) {
    return { stageCheckAuthorityEffect(key: string, companyId: string) { return nativeClearCommandAssertions(database, companyId, key); },
        stagePrepareGuidedIntakeEffect2(id: string, companyId: string) { return nativeClearCommandAssertions(database, companyId, id); } };
}
