import {companyAcceptanceReviewerProjection} from "@erpx/server-identity_access/public/company-acceptance-projection";
import {companyAcceptanceResultProjection} from "@erpx/server-organization/public/company-acceptance-result-projection";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function companyAcceptanceFacts(db:SqlDatabase){return {
reviewerNotifications(key:string,companyId:string,companyId2:string,recordVersion:number,now:number,companyId3:string,userId:string,now2:number,now3:number){return db.prepare("SELECT 1").bind();},
resultNotifications(key:string,value:"Company setup acceptance was approved" | "Company setup acceptance was returned for correction",now:number,companyId:string){return db.prepare("SELECT 1").bind();}
};}
