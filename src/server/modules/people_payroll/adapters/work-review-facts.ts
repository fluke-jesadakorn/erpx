import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
export function workReviewPeopleFacts(db:SqlDatabase){return {
 routePeople:(companyId:string,ids:string[])=>db.prepare(`SELECT user_id,legal_name FROM hr_people WHERE company_id=? AND user_id IN (${ids.map(()=>"?").join(",")}) AND merged_into_id IS NULL`).bind(companyId,...ids).all<{user_id:string;legal_name:string}>(),
 requester:(companyId:string,userId:string)=>db.prepare("SELECT legal_name FROM hr_people WHERE company_id=? AND user_id=? AND merged_into_id IS NULL LIMIT 1").bind(companyId,userId).first<{legal_name:string}>()
};}
