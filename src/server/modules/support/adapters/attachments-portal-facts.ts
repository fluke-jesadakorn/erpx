import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {assertion} from "../../workflow/public/commands";
export function portalAttachmentSupportFacts(db:SqlDatabase){return {
versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>targetType==="support_incident"?assertion(db,commandId,`attachment-owner-${index}`,"EXISTS(SELECT 1 FROM support_incidents WHERE id=? AND company_id=? AND version=?)",[targetId,companyId,version]):null
};}
