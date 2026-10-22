import "server-only";
import {randomUUID} from "node:crypto";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {iamMutationGuards} from "@erpx/server-identity_access/public/iam-store";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {workContextGuards} from "@erpx/server-workflow/public/commands";
import {commercePackageEnforcementMode,checkPurchasedCapability,purchaseCommandBoundary,observePurchaseResults,companyPurchaseReadLocks} from "@erpx/server-commercial/public/purchase-command-boundary";
import {financeDeferredEffectStatements} from "@erpx/server-finance/public/finance-transactions";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {CapabilityPurpose} from "@erpx/server-commercial/public/rules/packages";
import type {AttachmentOperation} from "@erpx/server-files/public/attachments/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import {landedCostPurchaseSourceRows,landedCostSourceReadLocks} from "@erpx/server-inventory/adapters/landed-cost-purchases";
export type LandedCostAttachmentPurchase={companyId:string;capability:"supply.advanced-inventory";purpose:CapabilityPurpose;actor:WorkContext;targetId:string;witness:string};
const image=`SELECT witness::text image FROM (${landedCostPurchaseSourceRows}) actual_landed_file WHERE id=? AND company_id=?`;
export async function landedCostAttachmentPurchaseScope(database:SqlDatabase,actor:WorkContext,type:string,id:string,purpose:CapabilityPurpose="settle-existing"):Promise<LandedCostAttachmentPurchase|null>{
 if(type!=="landed_cost"||await commercePackageEnforcementMode()==="disabled")return null;const witness=await database.prepare(image).bind(id,actor.companyId).first<string>("image");if(!witness)throw new ApiError(404,"ATTACHMENT_TARGET_NOT_FOUND","The original landed-cost receipt is unavailable.");return {companyId:actor.companyId,capability:"supply.advanced-inventory",purpose,actor,targetId:id,witness};
}
export async function authorizeLandedCostAttachmentPurchase(database:SqlDatabase,actor:WorkContext,type:string,id:string,operation:AttachmentOperation){
 const scope=await landedCostAttachmentPurchaseScope(database,actor,type,id,operation==="read"?"history":"settle-existing");if(!scope)return;
 const permission=operation==="read"?"inventory.read":"inventory.prepare",current=await database.prepare(`SELECT 1 FROM (SELECT s.*,NULL::text department_id FROM (${landedCostPurchaseSourceRows}) s) landed_file_scope WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?) AND ?::text IS NULL AND ${assignmentReadSql("landed_file_scope",permission)}`).bind(id,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.assignmentId,actor.userId,actor.role).first();if(!current)throw new ApiError(403,"LANDED_COST_ATTACHMENT_CHANGED","The original file assignment is unavailable.");
 for(const capability of ["supply.advanced-inventory","supply.receiving","supply.stock"])await checkPurchasedCapability(database,await commercePackageEnforcementMode(),{companyId:actor.companyId,capability,purpose:scope.purpose});
}
export async function landedCostAttachmentSourceReadLocks(database:SqlDatabase,scopes:readonly LandedCostAttachmentPurchase[]){
 if(!scopes.length||await commercePackageEnforcementMode()==="disabled")return [];
 const locks=[...new Set(scopes.map(scope=>scope.companyId))].sort().map(id=>companyPurchaseReadLocks(database,id));return [...locks.map(items=>items[0]),...landedCostSourceReadLocks(database),...locks.flatMap(items=>items.slice(1))];
}
export async function landedCostAttachmentCommandBoundary(database:SqlDatabase,scopes:readonly LandedCostAttachmentPurchase[]){
 const mode=await commercePackageEnforcementMode();if(mode==="disabled"||!scopes.length)return;const unique=[...new Map(scopes.map(scope=>[`${scope.companyId}:${scope.targetId}:${scope.purpose}`,scope])).values()],boundaries=unique.flatMap(scope=>["supply.advanced-inventory","supply.receiving","supply.stock"].map(capability=>{const key=`landed-cost-file-purchase:${randomUUID()}`;return {key,boundary:purchaseCommandBoundary(database,mode,key,{...scope,capability})!};}));
 const guards=(phase:string)=>unique.flatMap(scope=>{const actor=scope.actor,key=`landed-cost-file-${phase}:${randomUUID()}`;return [...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),assertion(database,key,"landed-cost-file-source",`EXISTS(SELECT 1 FROM (SELECT s.*,NULL::text department_id FROM (${landedCostPurchaseSourceRows}) s) landed_file_scope WHERE id=? AND company_id=? AND witness::text=? AND (?::text IS NULL OR branch_id=?) AND ?::text IS NULL AND ${assignmentReadSql("landed_file_scope","inventory.read")} AND ${assignmentReadSql("landed_file_scope","inventory.prepare")})`,[scope.targetId,scope.companyId,scope.witness,actor.branchId,actor.branchId,actor.departmentId,actor.assignmentId,actor.userId,actor.role,actor.assignmentId,actor.userId,actor.role]),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)];});
 return {before:[...boundaries.flatMap(item=>item.boundary.before),...guards("before")],after:[...financeDeferredEffectStatements(database),...guards("after"),...boundaries.flatMap(item=>item.boundary.after),...boundaries.map(item=>database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key))],observe:mode==="shadow"?observePurchaseResults:undefined};
}
export function landedCostAttachmentFailure(error:unknown){let current=error;for(let depth=0;depth<5&&current&&typeof current==="object";depth++){const item=current as {code?:string;detail?:string;cause?:unknown};if(item.code==="23514"&&item.detail?.includes("landed-cost-file-"))return new ApiError(409,"LANDED_COST_ATTACHMENT_CHANGED","The original landed-cost receipt or current file authority changed.");current=item.cause;}return null;}
