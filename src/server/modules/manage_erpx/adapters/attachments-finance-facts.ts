import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {assertion} from "../../workflow/public/commands";
import type {Row} from "../../files/public/contracts/attachments-finance";
export function FinanceAttachmentManageErpxFacts(db:SqlDatabase){return {
readOwner12:(facts:{companyId:string;targetId:string;targetType:string;purposeKey:string;})=>db.prepare("SELECT company_id,NULL::text branch_id,NULL::text department_id,version FROM company_provisioning WHERE company_id=? AND company_id=?").bind(facts.targetId,facts.companyId).first<Row>(),
versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>targetType==='company_provisioning'?assertion(db,commandId,`attachment-owner-${index}`,"EXISTS(SELECT 1 FROM company_provisioning WHERE company_id=? AND company_id=? AND version=?)",[targetId,companyId,version]):null,
versionStep:(targetType:string,targetId:string,expectedVersion:number,now:number)=>targetType==='company_provisioning'?db.prepare("UPDATE company_provisioning SET version=version+1,updated_at=? WHERE company_id=? AND version=?").bind(now,targetId,expectedVersion):null
};}
