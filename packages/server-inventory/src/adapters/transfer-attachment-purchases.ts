import "server-only";
import {randomUUID} from "node:crypto";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {iamMutationGuards} from "@erpx/server-identity_access/public/iam-store";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {workContextGuards} from "@erpx/server-workflow/public/commands";
import {commercePackageEnforcementMode,purchaseCommandBoundary,observePurchaseResults,checkPurchasedCapability} from "@erpx/server-commercial/public/purchase-command-boundary";
import type {CapabilityPurpose} from "@erpx/server-commercial/public/rules/packages";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {AttachmentOperation,AttachmentPolicy} from "@erpx/server-files/public/attachments/types";
import {financeDeferredEffectStatements} from "@erpx/server-finance/public/finance-transactions";
import {ApiError} from "@erpx/server-runtime/public/http";
import {requireTransferPurchasedRead,type TransferPurchaseTarget} from "@erpx/server-inventory/adapters/transfer-purchases";
export type TransferAttachmentPurchase={companyId:string;capability:"supply.transfers";purpose:CapabilityPurpose;actor:WorkContext;target:TransferPurchaseTarget;witness:string};
const kind=(type:string)=>type==="stock_transfer"||type==="inventory_transfer"?"transfer":type==="inventory_transit"?"transit":null;
const table=(target:TransferPurchaseTarget)=>target.kind==="transfer"?"inventory_transfers":"inventory_transit";
const image=(target:TransferPurchaseTarget)=>`SELECT (to_jsonb(transfer_file_source)-'version'-'updated_at')::text image FROM ${table(target)} transfer_file_source WHERE id=? AND company_id=?`;
export async function transferAttachmentPurchaseScope(database:SqlDatabase,actor:WorkContext,policy:Pick<AttachmentPolicy,"targetType"|"purposeKey">,id:string):Promise<TransferAttachmentPurchase|null>{
 const targetKind=kind(policy.targetType);if(!targetKind||await commercePackageEnforcementMode()==="disabled")return null;
 const target:TransferPurchaseTarget={kind:targetKind,id};
 const witness=await database.prepare(image(target)).bind(id,actor.companyId).first<string>("image");
 if(!witness)throw new ApiError(404,"ATTACHMENT_TARGET_NOT_FOUND","The original warehouse movement is unavailable.");
 return {companyId:actor.companyId,capability:"supply.transfers",purpose:policy.purposeKey==="dispatch_evidence"?"new-work":"settle-existing",actor,target,witness};
}
export async function authorizeTransferAttachmentPurchase(database:SqlDatabase,actor:WorkContext,policy:AttachmentPolicy,id:string,operation:AttachmentOperation){
 const targetKind=kind(policy.targetType),mode=await commercePackageEnforcementMode();if(!targetKind||mode==="disabled")return;
 await requireTransferPurchasedRead(database,actor,{kind:targetKind,id});
 await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:"supply.transfers",purpose:operation==="read"?"history":policy.purposeKey==="dispatch_evidence"?"new-work":"settle-existing"});
}
export async function transferAttachmentCommandBoundary(database:SqlDatabase,scopes:readonly TransferAttachmentPurchase[]){
 const mode=await commercePackageEnforcementMode();if(mode==="disabled"||!scopes.length)return undefined;
 const unique=[...new Map(scopes.map(scope=>[`${scope.companyId}:${scope.target.kind}:${scope.target.id}:${scope.purpose}`,scope])).values()];
 const boundaries=unique.map(scope=>{const key=`transfer-file:${randomUUID()}`;return {key,boundary:purchaseCommandBoundary(database,mode,key,scope)!};});
 const guards=(phase:string)=>unique.flatMap(scope=>{
  const key=`transfer-file-${phase}:${randomUUID()}`,actor=scope.actor,owner=table(scope.target);
  const authority=`EXISTS(SELECT 1 FROM (SELECT company_id,source_branch_id branch_id,NULL::text department_id FROM ${owner} WHERE id=? UNION ALL SELECT company_id,target_branch_id,NULL::text FROM ${owner} WHERE id=?) transfer_file_scope WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND ?::text IS NULL AND ${assignmentReadSql("transfer_file_scope","inventory.read")} AND (${assignmentReadSql("transfer_file_scope","inventory.prepare")} OR ${assignmentReadSql("transfer_file_scope","inventory.transfer.review")} OR ${assignmentReadSql("transfer_file_scope","finance.approve")}))`;
  return [database.prepare(`SELECT id FROM ${owner} WHERE id=? AND company_id=? FOR SHARE`).bind(scope.target.id,scope.companyId),database.prepare("SELECT id FROM assignments WHERE id=? AND company_id=? FOR SHARE").bind(actor.assignmentId,scope.companyId),...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),
   assertion(database,key,"transfer-file-owner",`(${image(scope.target)})=?`,[scope.target.id,scope.companyId,scope.witness]),
   assertion(database,key,"transfer-file-authority",authority,[scope.target.id,scope.target.id,scope.companyId,actor.branchId,actor.branchId,actor.departmentId,...Array.from({length:4},()=>[actor.assignmentId,actor.userId,actor.role]).flat()]),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)];
 });
 return {before:[...boundaries.flatMap(item=>item.boundary.before),...guards("before")],after:[...financeDeferredEffectStatements(database),...guards("after"),...boundaries.flatMap(item=>item.boundary.after),...boundaries.map(item=>database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key))],observe:mode==="shadow"?observePurchaseResults:undefined};
}
export function transferAttachmentFailure(error:unknown){let current=error;for(let depth=0;depth<5&&current&&typeof current==="object";depth++){const failure=current as {code?:string;detail?:string;cause?:unknown};if(failure.code==="23514"&&failure.detail?.includes("transfer-file-"))return new ApiError(409,"TRANSFER_ATTACHMENT_CHANGED","The warehouse movement or current file authority changed. Refresh before retrying.");current=failure.cause;}return null;}
