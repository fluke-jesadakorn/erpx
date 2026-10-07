import {companyAcceptanceLevelJoin,companyAcceptanceMissingLevel} from "../../operations/public/company-acceptance-level-projection";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
export function companyAcceptanceFacts(db:SqlDatabase){return {
async incompletePlacement(companyId:string){return db.prepare(`SELECT count(*) count FROM hr_engagements e
    JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id
    ${companyAcceptanceLevelJoin}
    WHERE e.company_id=? AND e.status='active' AND p.status='active' AND (${companyAcceptanceMissingLevel} OR e.position_id IS NULL)`)
    .bind(companyId).first<{ count: number }>();}
};}
