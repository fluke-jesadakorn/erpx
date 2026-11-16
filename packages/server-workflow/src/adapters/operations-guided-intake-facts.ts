import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function GuidedIntakeWorkflowFacts(database: SqlDatabase) {
    return { stageCheckAuthorityEffect(key: string) { return database.prepare("SELECT 1").bind(); },
        stagePrepareGuidedIntakeEffect2(id: string) { return database.prepare("SELECT 1").bind(); } };
}
