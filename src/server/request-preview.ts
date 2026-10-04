import "server-only";
import type { WorkContext } from "./authorization";
import type { ApprovalRoute } from "./staff-policy";
import type { SqlDatabase } from "./runtime/types";
import type { RequestPreview } from "@/lib/request-preview";
export async function namedRoute(db:SqlDatabase, actor:WorkContext, route:ApprovalRoute):Promise<RequestPreview>{
 const ids=route.steps.map(s=>s.userId);
 const people=ids.length?(await db.prepare(`SELECT user_id,legal_name FROM hr_people WHERE company_id=? AND merged_into_id IS NULL AND user_id IN (${ids.map(()=>"?").join(",")})`).bind(actor.companyId,...ids).all<{user_id:string;legal_name:string}>()).results:[];
 const names=new Map(people.map(p=>[p.user_id,p.legal_name]));
 return {steps:route.steps.map(s=>({label:(s.stage??s.authority).replaceAll("_"," "),person:names.get(s.userId)??"Eligible reviewer"})),blockers:route.blockedReason?[route.blockedReason]:[],setupPath:`/profiles/${encodeURIComponent(actor.userId)}`};
}
