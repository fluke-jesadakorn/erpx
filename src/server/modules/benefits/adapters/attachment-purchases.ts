import "server-only";
import {randomUUID} from "node:crypto";
import type {WorkContext} from "../../identity_access/public/authorization";
import {iamMutationGuards} from "../../identity_access/public/iam-store";
import {currentAssignmentReadScope} from "../../identity_access/public/record-source-authority";
import {hasPermission} from "@/lib/identity_access/permissions";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {ApiError} from "../../runtime/public/http";
import type {AttachmentOperation,AttachmentPolicy} from "../../files/public/attachments/types";
import {assertion,workContextGuards} from "../../workflow/public/commands";
import {financeDeferredEffectStatements} from "../../finance/public/finance-transactions";
import {peoplePackageEnforcementMode,checkPurchasedCapability,companyPurchaseReadLocks,purchaseCommandBoundary,observePurchaseResults} from "../../commercial/public/purchase-command-boundary";
import {requirePeopleCurrentAuthority} from "../../people_payroll/public/people-purchases";
import {benefitAttachmentPeopleSources as people} from "../../people_payroll/public/benefits-owner-facts";
import {benefitSourceReadLocks} from "./purchases";
const tables={benefit_plan:"benefit_plans",benefit_dependent:"benefit_dependents",benefit_election:"benefit_elections"} as const;
type Target=keyof typeof tables;
export type BenefitAttachmentPurchase={companyId:string;capability:"people.benefits";purpose:"new-work"|"settle-existing";actor:WorkContext;targetType:Target;targetId:string;purposeKey:string;witness:string};
const target=(type:string):type is Target=>Object.hasOwn(tables,type);
function source(type:Target){
 if(type==="benefit_plan")return `SELECT r.company_id,NULL::text branch_id,NULL::text department_id,r.status,NULL::text person_user_id,(to_jsonb(r)-'version'-'updated_at')::text image FROM benefit_plans r WHERE r.id=? AND r.company_id=?`;
 const engagement=type==="benefit_election"?`JOIN (${people.engagements}) e ON e.id=r.engagement_id AND e.company_id=r.company_id`:`LEFT JOIN LATERAL(SELECT x.* FROM (${people.engagements}) x WHERE x.person_id=p.id AND x.company_id=p.company_id ORDER BY CASE WHEN x.status='active' THEN 0 ELSE 1 END,x.start_date DESC,x.id LIMIT 1) e ON TRUE`;
 return `SELECT r.company_id,e.branch_id,e.department_id,r.status,p.user_id person_user_id,jsonb_build_object('record',to_jsonb(r)-'version'-'updated_at','person',to_jsonb(p),'engagement',to_jsonb(e))::text image FROM ${tables[type]} r JOIN (${people.people}) p ON p.id=r.person_id AND p.company_id=r.company_id ${engagement} WHERE r.id=? AND r.company_id=?`;
}
function access(actor:WorkContext,type:Target,purposeKey:string,read=false){
 const permission=read?"benefits.documents.read":"benefits.documents.manage",manager=hasPermission(actor,permission)?`iam_role_allows(?,?,'${permission}')`:"FALSE",args:(string|number|null)[]=hasPermission(actor,permission)?[actor.role,actor.companyId]:[];
 let self="FALSE";
 if(type==="benefit_plan"&&read){self=`(benefit_file_source.status IN ('published','retired') AND EXISTS(SELECT 1 FROM (${people.people}) p JOIN (${people.engagements}) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=benefit_file_source.company_id AND p.user_id=? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active'))`;args.push(actor.userId);}
 if(type!=="benefit_plan"){
  const phase=type==="benefit_dependent"?(read?"TRUE":"benefit_file_source.status='pending'"):purposeKey==="election_document"?(read?"TRUE":"benefit_file_source.status IN ('elected','submitted')"):read?"benefit_file_source.status IN ('acknowledged','released')":"FALSE";
  self=`(iam_same_person(benefit_file_source.person_user_id,?,benefit_file_source.company_id) AND ${phase})`;args.push(actor.userId);
 }
 return {where:`(${manager} OR ${self})`,args};
}
export async function benefitAttachmentPurchaseScope(database:SqlDatabase,actor:WorkContext,policy:Pick<AttachmentPolicy,"targetType"|"purposeKey">,targetId:string):Promise<BenefitAttachmentPurchase|null>{
 if(!target(policy.targetType)||await peoplePackageEnforcementMode()==="disabled")return null;
 const row=await database.prepare(source(policy.targetType)).bind(targetId,actor.companyId).first<{image:string}>();
 if(!row)throw new ApiError(404,"BENEFIT_ATTACHMENT_CHANGED","The actual Benefits evidence source is unavailable in this Company.");
 return {companyId:actor.companyId,capability:"people.benefits",purpose:policy.targetType==="benefit_plan"?"new-work":"settle-existing",actor,targetType:policy.targetType,targetId,purposeKey:policy.purposeKey,witness:row.image};
}
export async function authorizeBenefitAttachmentPurchase(database:SqlDatabase,actor:WorkContext,policy:AttachmentPolicy,targetId:string,operation:AttachmentOperation){
 const scope=await benefitAttachmentPurchaseScope(database,actor,policy,targetId);if(!scope)return;
 await requirePeopleCurrentAuthority(database,actor,`benefit-file-read:${randomUUID()}`);
 const allowed=access(actor,scope.targetType,scope.purposeKey,operation==="read"),placement=scope.targetType==="benefit_plan"?{where:"TRUE",args:[]}:{where:"(?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?)",args:[actor.branchId,actor.branchId,actor.departmentId,actor.departmentId]};
 if(!await database.prepare(`SELECT 1 FROM (${source(scope.targetType)}) benefit_file_source WHERE ${allowed.where} AND ${placement.where}`).bind(scope.targetId,scope.companyId,...allowed.args,...placement.args).first())throw new ApiError(404,"BENEFIT_ATTACHMENT_CHANGED","The current Benefits evidence policy no longer permits this source.");
 await checkPurchasedCapability(database,await peoplePackageEnforcementMode(),{companyId:scope.companyId,capability:scope.capability,purpose:operation==="read"?"history":scope.purpose});
}
/** Standalone binding advances the actual owner version. Writer leases are
 * acquired at Files atomic entry before the first Company/IAM snapshot. */
export async function benefitAttachmentSourceReadLocks(database:SqlDatabase,scopes:readonly BenefitAttachmentPurchase[]){
 if(!scopes.length||await peoplePackageEnforcementMode()==="disabled")return [];
 const locks=[...new Set(scopes.map(scope=>scope.companyId))].sort().map(company=>companyPurchaseReadLocks(database,company));
 return [...locks.map(items=>items[0]),...benefitSourceReadLocks(database),...locks.flatMap(items=>items.slice(1))];
}
export async function benefitAttachmentCommandBoundary(database:SqlDatabase,scopes:readonly BenefitAttachmentPurchase[]){
 const mode=await peoplePackageEnforcementMode();if(!scopes.length||mode==="disabled")return;
 const unique=[...new Map(scopes.map(scope=>[`${scope.companyId}:${scope.targetType}:${scope.targetId}:${scope.purposeKey}`,scope])).values()],boundaries=unique.map(scope=>{const key=`benefit-file-purchase:${randomUUID()}`;return {key,boundary:purchaseCommandBoundary(database,mode,key,scope)!};});
 const guards=(phase:string)=>unique.flatMap(scope=>{const actor=scope.actor,key=`benefit-file-${phase}:${randomUUID()}`,allowed=access(actor,scope.targetType,scope.purposeKey),current=currentAssignmentReadScope(actor,"workforce_actor_scope"),placement=scope.targetType==="benefit_plan"?{where:"TRUE",args:[]}:{where:"(?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?)",args:[actor.branchId,actor.branchId,actor.departmentId,actor.departmentId]};
 return [...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),assertion(database,key,"benefit-file-source",`EXISTS(SELECT 1 FROM (${source(scope.targetType)}) benefit_file_source WHERE image=? AND ${allowed.where} AND ${placement.where} AND EXISTS(SELECT 1 FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) workforce_actor_scope WHERE ${current.where}))`,[scope.targetId,scope.companyId,scope.witness,...allowed.args,...placement.args,actor.companyId,actor.branchId,actor.departmentId,...current.args]),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)];});
 return {before:[...boundaries.flatMap(item=>item.boundary.before),...guards("before")],after:[...financeDeferredEffectStatements(database),...guards("after"),...boundaries.flatMap(item=>item.boundary.after),...boundaries.map(item=>database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key))],observe:mode==="shadow"?observePurchaseResults:undefined};
}
export function benefitAttachmentFailure(error:unknown){let current=error;for(let depth=0;depth<5&&current&&typeof current==="object";depth++){const failure=current as {code?:string;detail?:string;cause?:unknown};if(failure.code==="23514"&&failure.detail?.includes("benefit-file-")){const mapped=new ApiError(409,"BENEFIT_ATTACHMENT_CHANGED","The actual Benefits evidence source or selected role changed. Refresh before retrying.");mapped.cause=current;return mapped;}current=failure.cause;}return null;}
