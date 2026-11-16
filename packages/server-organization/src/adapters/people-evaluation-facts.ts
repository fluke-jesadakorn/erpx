import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function peopleEvaluationOrganizationFacts(database: SqlDatabase) {
    return { readSaveEvaluationDefinitionPosition(positionId: string, companyId: string, levelId: string) { return database.prepare("SELECT id FROM organization_positions WHERE id=? AND company_id=? AND level_id=? AND active=1").bind(positionId, companyId, levelId).first(); },
        readSaveEvaluationDefinitionEffect(levelId: string, companyId: string) { return database.prepare("SELECT 1 FROM staff_levels WHERE id=? AND company_id=? AND active=1").bind(levelId, companyId).first(); },
        readEvaluationWorkspacePositions(companyId: string) { return database.prepare("SELECT id,title,level_id FROM organization_positions WHERE company_id=? AND active=1 ORDER BY title").bind(companyId).all<{
            id: string;
            title: string;
            level_id: string;
        }>(); },
        readEvaluationWorkspaceLevels(companyId: string) { return database.prepare("SELECT id,name FROM staff_levels WHERE company_id=? AND active=1 ORDER BY rank").bind(companyId).all<{
            id: string;
            name: string;
        }>(); },
        readImportEvaluationMarketLevels(companyId: string) { return database.prepare("SELECT id FROM staff_levels WHERE company_id=? AND active=1").bind(companyId).all<{
            id: string;
        }>(); } };
}
