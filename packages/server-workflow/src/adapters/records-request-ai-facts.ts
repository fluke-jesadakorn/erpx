import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function RequestAiWorkflowFacts(database: SqlDatabase) {
    return { stageRunRequestAiEffect(authorizationKey: string) { return database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(authorizationKey); },
        stageRunRequestAiEffect4(command: string) {
            return database.prepare("DELETE FROM command_assertions WHERE command_id=?")
                .bind(command);
        } };
}
