import { commercialOwnerProjectionSources as crmProjectionSources } from "../../crm/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as pmsProjectionSources } from "../../pms/public/commercial-owner-projection-sources";
import "server-only";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
export function prepareCrmSourceTouch(db:SqlDatabase,a:{companyId:string;userId:string},input:{id:string;key:string;campaignId?:string;source:string;sourceEvidence?:string;sourceKind?:string;sourceId?:string;now:number}){
 return db.prepare("INSERT INTO marketing_touches(id,company_id,lead_id,campaign_id,source,evidence,source_kind,source_id,occurred_at,created_by) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(`touch-${input.key}`,a.companyId,input.id,input.campaignId??null,input.source,input.sourceEvidence??input.source,input.sourceKind??"staff",input.sourceId??input.id,input.now,a.userId);
}
export function prepareCrmReservationTouch(db:SqlDatabase,a:{companyId:string;userId:string},input:{id:string;key:string;reservationId:string}){
 return db.prepare(`INSERT INTO marketing_touches(id,company_id,lead_id,campaign_id,source,evidence,source_kind,source_id,occurred_at,created_by) SELECT ?,?,l.id,c.id,'PMS reservation','Staff-entered campaign code on confirmed PMS reservation','pms',r.id,r.updated_at,? FROM (${pmsProjectionSources.pms_records}) r JOIN (${crmProjectionSources.crm_leads}) l ON l.id=? JOIN marketing_campaigns c ON c.company_id=l.company_id AND c.code=r.data_json::jsonb->>'campaignCode' WHERE r.id=? AND r.kind='reservation' AND l.company_id=? ON CONFLICT(lead_id,source_kind,source_id) DO NOTHING`).bind(`touch-pms-${input.key}`,a.companyId,a.userId,input.id,input.reservationId,a.companyId);
}
