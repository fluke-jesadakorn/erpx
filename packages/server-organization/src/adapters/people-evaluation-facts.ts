import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function peopleEvaluationOrganizationFacts(database: SqlDatabase) {
    return { readSaveEvaluationDefinitionPosition(positionId: string, companyId: string, levelId: string) { return database.prepare("SELECT 1").bind().first(); },
        readSaveEvaluationDefinitionEffect(levelId: string, companyId: string) { return database.prepare("SELECT 1").bind().first(); },
        readEvaluationWorkspacePositions(companyId: string) { return database.prepare("SELECT 1").bind().all<{
            id: string;
            title: string;
            level_id: string;
        }>(); },
        readEvaluationWorkspaceLevels(companyId: string) { return database.prepare("SELECT 1").bind().all<{
            id: string;
            name: string;
        }>(); },
        readImportEvaluationMarketLevels(companyId: string) { return database.prepare("SELECT 1").bind().all<{
            id: string;
        }>(); } };
}
