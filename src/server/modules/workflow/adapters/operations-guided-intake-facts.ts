import "server-only";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
export function GuidedIntakeWorkflowFacts(database: SqlDatabase) {
    return { stageCheckAuthorityEffect(key: string) { return database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key); },
        stagePrepareGuidedIntakeEffect2(id: string) { return database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(id); } };
}
