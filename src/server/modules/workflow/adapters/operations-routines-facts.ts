import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
export function RoutinesWorkflowFacts(database: SqlDatabase) {
    return { stageGenerateOccurrencesEffect4(key: string) {
            return database.prepare("DELETE FROM command_assertions WHERE command_id=?")
                .bind(key);
        } };
}
