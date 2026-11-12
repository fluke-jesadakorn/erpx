import "server-only";
import {randomUUID} from "node:crypto";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {loadIamAuthority} from "@erpx/server-identity_access/public/iam-store";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {hasModuleAccess} from "@erpx/catalogue/module-access";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {ApiError} from "@erpx/server-runtime/public/http";
import {peoplePackageEnforcementMode,readPurchasedAccess} from "@erpx/server-commercial/public/purchase-command-boundary";
import {requirePeopleCurrentAuthority} from "@erpx/server-people_payroll/public/people-purchases";
import {benefitPeopleSourceLocks} from "@erpx/server-people_payroll/public/benefits-owner-facts";
import {benefitEligibilitySourceLocks} from "@erpx/server-learning/public/benefit-source-facts";

export type BenefitPurchaseControls={newWork:boolean;settleExisting:boolean;history:boolean;revision:string};
const permission=(review:boolean)=>review?"benefits.review":"records.write";

export const benefitAssignmentModulePredicate="((selected.role LIKE 'custom_%' AND iam_role_allows(selected.role,selected.company_id,'module.benefits')) OR (selected.role NOT LIKE 'custom_%' AND (jsonb_array_length(selected.modules_json::jsonb)=0 OR selected.modules_json::jsonb @> jsonb_build_array('benefits'::text))))";
async function current(database:SqlDatabase,actor:WorkContext,key:string,write:boolean,review=false){
 if(!hasModuleAccess(actor.modules,"benefits"))throw new ApiError(403,"MODULE_FORBIDDEN","Your current role cannot access Benefits.");
 await requirePeopleCurrentAuthority(database,actor,key,write?"records.write":"records.read");
 if(!await database.prepare(`SELECT 1 FROM assignments selected WHERE selected.id=? AND ${benefitAssignmentModulePredicate}`).bind(actor.assignmentId).first())throw new ApiError(403,"BENEFIT_ACCESS_CHANGED","The current assignment no longer includes Benefits.");
 if(review)await requirePeopleCurrentAuthority(database,actor,key,permission(true));
}
export function benefitSourceReadLocks(database:SqlDatabase){
 return [database.prepare("LOCK TABLE benefit_plans,benefit_plan_events,benefit_enrollment_windows,benefit_window_events,benefit_dependents,benefit_dependent_events,benefit_elections,benefit_election_events,benefit_payroll_inputs IN SHARE ROW EXCLUSIVE MODE"),...benefitPeopleSourceLocks(database),...benefitEligibilitySourceLocks(database)];
}
export async function benefitPurchaseControls(actor:WorkContext,provided?:SqlDatabase):Promise<BenefitPurchaseControls|undefined>{
 const mode=await peoplePackageEnforcementMode();if(mode==="disabled")return undefined;
 const database=provided??(await holdingResources(actor.holdingId)).write;
 await current(database,actor,`benefit-read:${randomUUID()}`,false);
 const access=await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:"people.benefits"});
 return {newWork:access.newWork,settleExisting:access.settleExisting,history:access.history,revision:access.revision};
}
export async function withBenefitPurchasedRead<Result>(actor:WorkContext,run:(database:SqlDatabase,live:WorkContext,purchase?:BenefitPurchaseControls)=>Promise<Result>,empty?:()=>Result){
 const database=(await holdingResources(actor.holdingId)).write,mode=await peoplePackageEnforcementMode();
 if(mode==="disabled")return run(database,actor);
 if(empty&&(!hasModuleAccess(actor.modules,"benefits")||!hasPermission(actor,"records.read")))return empty();
 const purchase=await benefitPurchaseControls(actor,database);
 if(purchase&&!purchase.history){if(empty)return empty();throw new ApiError(403,"PACKAGE_REQUIRED","This Benefits work requires its purchased capability or retained history.");}
 const live=await loadIamAuthority(database,actor.companyId,actor.userId,actor.role,actor.authority?.sessionId);
 const authorized={...actor,iam:{...live,permissions:live.permissions.filter(code=>hasPermission(actor,code)),grantablePermissions:live.grantablePermissions.filter(code=>hasPermission(actor,code))}};
 return run(database,authorized,purchase);
}
