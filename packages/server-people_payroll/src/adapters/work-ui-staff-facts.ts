import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function workUiStaffFacts(db:SqlDatabase){return {
async headPlacement(companyId:string,value:string,personId:string){return db.prepare("SELECT e.position_id FROM hr_engagements e WHERE e.company_id=? AND e.department_id=? AND e.person_id=? AND e.status='active' ORDER BY e.updated_at DESC LIMIT 1").bind(companyId,value,personId).first<{position_id:string|null}>();}
};}
