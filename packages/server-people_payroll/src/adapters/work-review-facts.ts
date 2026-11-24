import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ppRd40UnlinkedUserByCompany} from "./pp-read-tail-r40-sources";
export function workReviewPeopleFacts(db:SqlDatabase){return {
 routePeople:(companyId:string,ids:string[])=>db.prepare(`SELECT user_id,legal_name FROM hr_people WHERE company_id=? AND user_id IN (${ids.map(()=>"?").join(",")}) AND merged_into_id IS NULL`).bind(companyId,...ids).all<{user_id:string;legal_name:string}>(),
 requester:(companyId:string,userId:string)=>db.prepare(ppRd40UnlinkedUserByCompany('legal_name')).bind(companyId,userId).first<{legal_name:string}>()
};}
