import {expenseCurrentAuthorityImage,requireExpensePurchasedHistory} from "@erpx/server-people_payroll/adapters/expense-purchases";
import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {AttachmentOperation} from "@erpx/server-files/public/attachments/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {workContextGuards} from '@erpx/server-workflow/public/command-guards';
import {iamMutationGuards} from "@erpx/server-identity_access/public/iam-store";
import {checkPurchasedCapability,peoplePackageEnforcementMode,purchaseCommandBoundary,observePurchaseResults} from "@erpx/server-commercial/public/purchase-command-boundary";
import type {CapabilityPurpose} from "@erpx/server-commercial/public/rules/packages";
import {financeDeferredEffectStatements} from "@erpx/server-finance/public/finance-transactions";
import {requirePayrollCurrentAuthority} from "@erpx/server-people_payroll/adapters/payroll-purchases";
import {ApiError} from "@erpx/server-runtime/public/http";

const tables={payroll_run:"hr_payroll_runs",payroll_advance:"hr_payroll_advances",payroll_final_pay:"hr_payroll_final_pay_preparations",expense_claim:"finance_expenses",expense_payment:"expense_payment_drafts"} as const;
type Target=keyof typeof tables;
export type PayrollAttachmentPurchase={companyId:string;capability:"people.payroll"|"people.expenses";purpose:CapabilityPurpose;actor:WorkContext;targetType:Target;targetId:string;sourceImage:string;authorityImage?:string};
const capability=(targetType:Target)=>targetType==="expense_claim"||targetType==="expense_payment"?"people.expenses":"people.payroll";
function target(value:string):value is Target{return Object.hasOwn(tables,value);}
/** Only the existing sensitive owner policy may authorize a file. The witness
 * retains actual owner/employee/clearance identity through deferred effects;
 * the original Files record bind may increment version and updated_at. */
function source(targetType:Target){
 if(targetType==="expense_claim")return `SELECT jsonb_build_object('record',to_jsonb(t)-'version'-'updated_at')::text image FROM finance_expenses t WHERE t.id=? AND t.company_id=?`;
 if(targetType==="expense_payment")return `SELECT jsonb_build_object('record',to_jsonb(t)-'version'-'updated_at','claim',to_jsonb(e)-'version'-'updated_at')::text image FROM expense_payment_drafts t JOIN finance_expenses e ON e.id=t.expense_id AND e.company_id=t.company_id WHERE t.id=? AND t.company_id=?`;
 const engagement=targetType==="payroll_run"?"":` JOIN hr_engagements e ON e.id=t.engagement_id AND e.company_id=t.company_id JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id`;
 const task=targetType==="payroll_final_pay"?" JOIN people_preparation_tasks task ON task.id=t.task_id AND task.company_id=t.company_id":"";
 const related=targetType==="payroll_run"?"":`, 'engagement',jsonb_build_object('id',e.id,'personId',e.person_id,'companyId',e.company_id,'branchId',e.branch_id,'departmentId',e.department_id,'status',e.status),'person',jsonb_build_object('id',p.id,'companyId',p.company_id,'userId',p.user_id,'status',p.status,'mergedIntoId',p.merged_into_id)`;
 const clearance=targetType==="payroll_final_pay"?",'task',jsonb_build_object('id',task.id,'companyId',task.company_id,'status',task.status,'assigneeId',task.assignee_id,'completedBy',task.completed_by,'clearanceType',task.clearance_type)":"";
 return `SELECT jsonb_build_object('record',to_jsonb(t)-'version'-'updated_at'${related}${clearance})::text image FROM ${tables[targetType]} t${engagement}${task} WHERE t.id=? AND t.company_id=?`;
}
export async function payrollAttachmentPurchaseScope(database:SqlDatabase,actor:WorkContext,targetType:string,targetId:string,purpose:CapabilityPurpose="settle-existing"):Promise<PayrollAttachmentPurchase|null>{
 if(!target(targetType)||await peoplePackageEnforcementMode()==="disabled")return null;
 const image=await database.prepare(source(targetType)).bind(targetId,actor.companyId).first<string>("image");
 if(!image)throw new ApiError(409,"PAYROLL_ATTACHMENT_SOURCE_CHANGED","The actual Payroll file owner changed. Refresh its retained source.");
 return {companyId:actor.companyId,capability:capability(targetType),purpose,actor,targetType,targetId,sourceImage:image,...(capability(targetType)==="people.expenses"?{authorityImage:await expenseCurrentAuthorityImage(database,actor)}:{})};
}
export async function authorizePayrollAttachmentPurchase(database:SqlDatabase,actor:WorkContext,targetType:string,operation:AttachmentOperation){
 if(!target(targetType))return;
 const mode=await peoplePackageEnforcementMode();if(mode==="disabled")return;
 await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:capability(targetType),purpose:operation==="read"?"history":"settle-existing"});
 if(capability(targetType)==="people.expenses")await requireExpensePurchasedHistory(database,actor);else await requirePayrollCurrentAuthority(database,actor,`payroll-file-read:${crypto.randomUUID()}`);
}
/** Original Files transaction and guards remain authoritative. Private source
 * witnesses never become client purchase controls or a role/Company grant. */
export async function payrollAttachmentCommandBoundary(database:SqlDatabase,scopes:readonly PayrollAttachmentPurchase[]){
 const mode=await peoplePackageEnforcementMode();if(!scopes.length||mode==="disabled")return undefined;
 const unique=[...new Map(scopes.map(scope=>[`${scope.companyId}:${scope.capability}:${scope.purpose}`,scope])).values()].sort((a,b)=>a.companyId.localeCompare(b.companyId)||a.purpose.localeCompare(b.purpose));
 const boundaries=unique.map(scope=>{const key=`payroll-file-purchase:${crypto.randomUUID()}`;return {key,boundary:purchaseCommandBoundary(database,mode,key,scope)!};});
 const actors=[...new Map(scopes.map(scope=>[scope.actor.assignmentId,scope.actor])).values()];
 const guards=(phase:string)=>[
  ...actors.flatMap(actor=>{const key=`payroll-file-${phase}:${crypto.randomUUID()}`;return [database.prepare("SELECT id FROM assignments WHERE id=? AND company_id=? FOR SHARE").bind(actor.assignmentId,actor.companyId),...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),assertion(database,key,"payroll-file-assignment",`EXISTS(SELECT 1 FROM assignments WHERE id=? AND company_id=? AND user_id=? AND role=? AND valid_from<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND valid_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND revoked_at IS NULL AND (?::text IS NULL OR branch_id IS NULL OR branch_id=?))`,[actor.assignmentId,actor.companyId,actor.userId,actor.role,actor.branchId,actor.branchId]),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)];}),
  ...scopes.flatMap(scope=>{const key=`payroll-file-source-${phase}:${crypto.randomUUID()}`,owners=scope.targetType==="payroll_run"||scope.targetType==="expense_claim"?"t":scope.targetType==="expense_payment"?"t,e":scope.targetType==="payroll_final_pay"?"t,e,p,task":"t,e,p";return [database.prepare(`${source(scope.targetType)} FOR SHARE OF ${owners}`).bind(scope.targetId,scope.companyId),assertion(database,key,"payroll-file-owner",`(${source(scope.targetType)})=?`,[scope.targetId,scope.companyId,scope.sourceImage]),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)];}),
 ];
 return {before:[...boundaries.flatMap(item=>item.boundary.before),...guards("before")],after:[...financeDeferredEffectStatements(database),...guards("after"),...boundaries.flatMap(item=>item.boundary.after),...boundaries.map(item=>database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key))],observe:mode==="shadow"?observePurchaseResults:undefined};
}
/** The actual Files unit invokes this after ALL metadata/audit/cleanup writes
 * and its deferred flush. No assertion, cleanup or receipt is written here. */
export async function assertExpenseAttachmentPurchases(database:SqlDatabase,scopes:readonly PayrollAttachmentPurchase[]){
 const mode=await peoplePackageEnforcementMode();if(mode==='disabled')return;
 for(const scope of scopes){if(scope.capability!=='people.expenses')continue;
  await checkPurchasedCapability(database,mode,scope);await requireExpensePurchasedHistory(database,scope.actor);
  if(!scope.authorityImage||await expenseCurrentAuthorityImage(database,scope.actor)!==scope.authorityImage)throw new ApiError(403,"EXPENSE_ACCESS_CHANGED","The original Expense evidence authority changed before commit.");
  if(await database.prepare(source(scope.targetType)).bind(scope.targetId,scope.companyId).first<string>('image')!==scope.sourceImage)throw new ApiError(409,'EXPENSE_ATTACHMENT_SOURCE_CHANGED','The actual Expense evidence owner changed before commit.');
 }
}
