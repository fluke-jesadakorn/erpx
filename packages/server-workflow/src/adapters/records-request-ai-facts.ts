import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function RequestAiWorkflowFacts(database: SqlDatabase) {
    return { stageRunRequestAiEffect(authorizationKey: string) { return database.prepare("SELECT 1").bind(); },
        stageRunRequestAiEffect4(command: string) {
            return database.prepare("SELECT 1")
                .bind(command);
        } };
}
