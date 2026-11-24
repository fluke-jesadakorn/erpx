import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {activeIdByCompanySource} from './org-sources';
import {activeStaffLevelSource} from './organization-read-tail-sources-r39';
import {activePositionList,activeStaffLevels} from './organization-read-tail-sources-r40';
export function peopleEvaluationOrganizationFacts(database: SqlDatabase) {
    return { readSaveEvaluationDefinitionPosition(positionId: string, companyId: string, levelId: string) { return database.prepare("SELECT id FROM organization_positions WHERE id=? AND company_id=? AND level_id=? AND active=1").bind(positionId, companyId, levelId).first(); },
        readSaveEvaluationDefinitionEffect(levelId: string, companyId: string) { return database.prepare(activeStaffLevelSource).bind(levelId, companyId).first(); },
        readEvaluationWorkspacePositions(companyId: string) { return database.prepare(activePositionList('id,title,level_id','ORDER BY title')).bind(companyId).all<{
            id: string;
            title: string;
            level_id: string;
        }>(); },
        readEvaluationWorkspaceLevels(companyId: string) { return database.prepare(activeStaffLevels('ORDER BY rank')).bind(companyId).all<{
            id: string;
            name: string;
        }>(); },
        readImportEvaluationMarketLevels(companyId: string) { return database.prepare(activeIdByCompanySource('staff_levels')).bind(companyId).all<{
            id: string;
        }>(); } };
}
