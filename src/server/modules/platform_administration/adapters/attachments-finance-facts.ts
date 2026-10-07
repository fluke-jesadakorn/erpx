import "server-only";
import type {SqlDatabase,SqlStatement} from "../../runtime/public/runtime/types";
import {assertion} from "../../workflow/public/commands";
import type {Row} from "../../files/public/contracts/attachments-finance";
const target = "company_release_acceptance";
export function FinanceAttachmentPlatformAdministrationFacts(db:SqlDatabase){return {
 readOwner10:(facts:{companyId:string;targetId:string;targetType:string;purposeKey:string;})=>db.prepare("SELECT company_id,NULL::text branch_id,NULL::text department_id,version FROM company_release_acceptances WHERE id=? AND company_id=?").bind(facts.targetId,facts.companyId).first<Row>(),
 versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number):SqlStatement|null=>targetType===target?assertion(db,commandId,`attachment-owner-${index}`,"EXISTS(SELECT 1 FROM company_release_acceptances WHERE id=? AND company_id=? AND version=?)",[targetId,companyId,version]):null,
 versionStep:(targetType:string,targetId:string,expectedVersion:number,now:number):SqlStatement|null=>targetType===target?db.prepare("UPDATE company_release_acceptances SET version=version+1,updated_at=? WHERE id=? AND version=?").bind(now,targetId,expectedVersion):null,
};}
