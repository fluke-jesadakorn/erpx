import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function prepareCrmSourceTouch(db:SqlDatabase,a:{companyId:string;userId:string},input:{id:string;key:string;campaignId?:string;source:string;sourceEvidence?:string;sourceKind?:string;sourceId?:string;now:number}){
 return db.prepare("INSERT INTO marketing_touches(id,company_id,lead_id,campaign_id,source,evidence,source_kind,source_id,occurred_at,created_by) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(`touch-${input.key}`,a.companyId,input.id,input.campaignId??null,input.source,input.sourceEvidence??input.source,input.sourceKind??"staff",input.sourceId??input.id,input.now,a.userId);
}
