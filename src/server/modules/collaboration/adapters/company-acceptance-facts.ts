import {companyAcceptanceReviewerProjection} from "../../identity_access/public/company-acceptance-projection";
import {companyAcceptanceResultProjection} from "../../organization/public/company-acceptance-result-projection";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
export function companyAcceptanceFacts(db:SqlDatabase){return {
reviewerNotifications(key:string,companyId:string,companyId2:string,recordVersion:number,now:number,companyId3:string,userId:string,now2:number,now3:number){return db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
      ${companyAcceptanceReviewerProjection} ON CONFLICT DO NOTHING`).bind(key, companyId, companyId2, recordVersion, now, companyId3, userId, now2, now3);},
resultNotifications(key:string,value:"Company setup acceptance was approved" | "Company setup acceptance was returned for correction",now:number,companyId:string){return db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) ${companyAcceptanceResultProjection} ON CONFLICT DO NOTHING`).bind(key, value, now, companyId);}
};}
