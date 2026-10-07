import type {SqlDatabase} from "../../runtime/public/runtime/types";
import type {ScheduleRow} from "../public/routines";
export function workUiStaffFacts(db:SqlDatabase){return {
async templateSchedules(value:string,companyId:string){return db
                    .prepare(
                      "SELECT * FROM routine_schedules WHERE template_id=? AND company_id=? ORDER BY created_at",
                    )
                    .bind(value, companyId)
                    .all<ScheduleRow>();}
};}
