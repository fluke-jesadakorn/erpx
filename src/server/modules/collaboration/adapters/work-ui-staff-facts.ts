import type {SqlDatabase} from "../../runtime/public/runtime/types";
export function workUiStaffFacts(db:SqlDatabase){return {
notificationRead(now:number,value:string,companyId:string,userId:string){return db
            .prepare(
              "UPDATE staff_notifications SET read_at=? WHERE id=? AND company_id=? AND user_id=?",
            )
            .bind(now, value, companyId, userId)
            .run();}
};}
