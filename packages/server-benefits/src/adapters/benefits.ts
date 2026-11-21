import {activeBenefitPerson,benefitElectionEmployeeName} from "@erpx/server-people_payroll/public/benefits-owner-facts";
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { hasModuleAccess } from "@erpx/catalogue/module-access";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { BusinessRuleError } from "@erpx/server-shared/index";
import { createBenefitsApplication, type BenefitsPorts } from "@erpx/server-benefits/application/benefits";
import {type BenefitRow} from "@erpx/server-benefits/domain/benefits";
import {withBenefitPurchasedRead} from "@erpx/server-benefits/adapters/purchases";
import {benefitPresentationContext} from "@erpx/server-benefits/adapters/presentation";
function access(a:WorkContext){requirePermission(a,"records.read");if(!hasModuleAccess(a.modules,"benefits"))throw new ApiError(403,"MODULE_FORBIDDEN","Your current role cannot access Benefits.");}
function application(a:WorkContext,provided?:SqlDatabase){
 let resource:Promise<SqlDatabase>|null=null;const database=()=>provided?Promise.resolve(provided):resource??=holdingResources(a.holdingId).then(x=>x.write);
 const person=async()=>activeBenefitPerson(await database(),a.companyId,a.userId);
 const rows=async(sql:string,...bind:(string|number|boolean|null)[])=>((await database()).prepare(sql).bind(...bind).all<BenefitRow>()).then(x=>x.results);
 const ports:BenefitsPorts={
 actorId:a.userId,canRead:hasModuleAccess(a.modules,"benefits")&&hasPermission(a,"records.read"),canReview:hasPermission(a,"benefits.review"),canWrite:hasPermission(a,"records.write"),canManageDocuments:hasPermission(a,"benefits.documents.manage"),
 async authorize(){access(a);},
 person,
 plans:()=>rows("SELECT * FROM benefit_plans WHERE company_id=? ORDER BY updated_at DESC",a.companyId),
 windows:()=>rows("SELECT * FROM benefit_enrollment_windows WHERE company_id=? ORDER BY updated_at DESC",a.companyId),
 dependents:(personId)=>rows("SELECT * FROM benefit_dependents WHERE company_id=? AND person_id=? ORDER BY updated_at DESC",a.companyId,personId),
 elections:(personId,reviewer,activeOnly=false)=>rows(`SELECT e.*,p.name plan_name,(${benefitElectionEmployeeName()}) employee_name FROM benefit_elections e JOIN benefit_plans p ON p.id=e.plan_id WHERE e.company_id=? ${activeOnly?"AND e.status NOT IN ('released','cancelled')":""} AND (? OR e.person_id=?) ORDER BY e.updated_at DESC`,a.companyId,reviewer,personId),
 election:async(id,personId,reviewer)=>(await database()).prepare(`SELECT e.*,p.name plan_name,p.version plan_current_version,p.status plan_status,(${benefitElectionEmployeeName()}) employee_name FROM benefit_elections e JOIN benefit_plans p ON p.id=e.plan_id WHERE e.id=? AND e.company_id=? AND (? OR e.person_id=?)`).bind(id,a.companyId,reviewer,personId).first<BenefitRow>(),
 history:(id)=>rows("SELECT * FROM benefit_election_events WHERE election_id=? ORDER BY version",id),
 payroll:async(id)=>(await database()).prepare("SELECT * FROM benefit_payroll_inputs WHERE election_id=?").bind(id).first<BenefitRow>(),
 };return createBenefitsApplication(ports);
}
async function translated<Result>(action:()=>Promise<Result>):Promise<Result>{try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function benefitsWorkspace(a:WorkContext,personal=false){return translated(()=>withBenefitPurchasedRead(a,async(database,live,purchase)=>({...await application(live,database).workspace(personal),workContext:benefitPresentationContext(live),...(purchase?{purchase}:{})})));}
export function benefitDetail(a:WorkContext,id:string){return translated(()=>withBenefitPurchasedRead(a,async(database,live,purchase)=>({...await application(live,database).detail(id),canWrite:hasPermission(live,"records.write"),workContext:benefitPresentationContext(live),...(purchase?{purchase}:{})})));}
export function benefitWork(a:WorkContext){return translated(()=>withBenefitPurchasedRead(a,async(database,live,purchase)=>(await application(live,database).work()).map(item=>purchase&&!purchase.settleExisting?{...item,canAct:false,queue:"waiting" as const,relationships:item.relationships?.filter(value=>value!=="approval")}:item),()=>[]));}
