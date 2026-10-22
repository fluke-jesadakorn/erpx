import "server-only";
import {randomUUID} from "node:crypto";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {AttachmentOperation,AttachmentPolicy} from "@erpx/server-files/public/attachments/types";
import type {CapabilityPurpose} from "@erpx/server-commercial/public/rules/packages";
import {commercePackageEnforcementMode,checkPurchasedCapability,purchaseCommandBoundary,observePurchaseResults} from "@erpx/server-commercial/public/purchase-command-boundary";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {iamMutationGuards} from "@erpx/server-identity_access/public/iam-store";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {workContextGuards} from "@erpx/server-workflow/public/commands";
import {financeDeferredEffectStatements} from "@erpx/server-finance/public/finance-transactions";
import {ApiError} from "@erpx/server-runtime/public/http";
import {commerceKindCapability,commercePurchaseSource,requireCommerceCurrentAuthority} from "@erpx/server-commerce/adapters/purchase-boundary";

type Target={type:"order"|"fulfillment"|"return";id:string};
export type CommerceAttachmentPurchase={companyId:string;capability:string;purpose:CapabilityPurpose;actor:WorkContext;target:Target;source:{id:string;company_id:string;branch_id:string|null;department_id:string|null;kind:"quote"|"sales"|"purchase"};witness:string;permissions:readonly string[]};
function targetFor(type:string,id:string):Target|null{
 return ["commerce_order","commerce_order_delivery","purchase_order","purchase_receipt"].includes(type)?{type:"order",id}:type==="commerce_fulfillment"?{type:"fulfillment",id}:type==="commerce_return"?{type:"return",id}:null;
}
const table=(target:Target)=>target.type==="order"?"commerce_orders":target.type==="fulfillment"?"commerce_fulfillments":"commerce_returns";
const image=(target:Target)=>`(SELECT to_jsonb(commerce_file_target)-'version'-'updated_at' FROM ${table(target)} commerce_file_target WHERE commerce_file_target.id=?)`;
function permissions(policy:Pick<AttachmentPolicy,"targetType"|"purposeKey">,kind:string){
 if(policy.targetType==="commerce_order_delivery")return ["inventory.prepare"];
 if(policy.targetType==="purchase_receipt"||policy.targetType==="purchase_order"&&["delivery_note","packing_list","damage_photo"].includes(policy.purposeKey)||policy.targetType==="commerce_order"&&policy.purposeKey==="purchase_receipt")return ["inventory.prepare","commerce.purchase"];
 return [kind==="purchase"?"commerce.purchase":"commerce.sales"];
}
/** Original Files policy runs first. Only persisted Commerce targets resolve a
 * purchase: no product, warehouse, party, display label or caller source type. */
export async function commerceAttachmentPurchaseScope(database:SqlDatabase,actor:WorkContext,policy:Pick<AttachmentPolicy,"targetType"|"purposeKey">,targetId:string,purpose:CapabilityPurpose="settle-existing"):Promise<CommerceAttachmentPurchase|null>{
 const target=targetFor(policy.targetType,targetId);if(!target||await commercePackageEnforcementMode()==="disabled")return null;
 const source=await commercePurchaseSource(database,actor,target);if(!source)throw new ApiError(404,"ATTACHMENT_TARGET_NOT_FOUND","The original Commerce file source was not found.");
 const witness=await database.prepare(`SELECT ${image(target)}::text witness`).bind(target.id).first<string>("witness");
 if(!witness)throw new ApiError(404,"ATTACHMENT_TARGET_NOT_FOUND","The original Commerce file source was not found.");
 return {companyId:source.company_id,capability:commerceKindCapability(source.kind),purpose,actor,target,source,witness,permissions:permissions(policy,source.kind)};
}
export async function authorizeCommerceAttachmentPurchase(database:SqlDatabase,actor:WorkContext,policy:AttachmentPolicy,targetId:string,operation:AttachmentOperation){
 const scope=await commerceAttachmentPurchaseScope(database,actor,policy,targetId,operation==="read"?"history":"settle-existing");if(!scope)return;
 await checkPurchasedCapability(database,await commercePackageEnforcementMode(),scope);
 if(operation==="read")await requireCommerceCurrentAuthority(database,actor,["commerce.read"],scope.source,`commerce-file-read:${randomUUID()}`);
}
/** File bytes/Legal holds/scanner/owner versions remain Files responsibilities.
 * Current assignment, exact source binding and purchases fence its same atomic
 * receipt/audit/outbox batch, after all deferred Finance owner effects. */
export async function commerceAttachmentCommandBoundary(database:SqlDatabase,scopes:readonly CommerceAttachmentPurchase[]){
 const mode=await commercePackageEnforcementMode();if(!scopes.length||mode==="disabled")return undefined;
 const unique=[...new Map(scopes.map(scope=>[`${scope.companyId}:${scope.target.type}:${scope.target.id}:${scope.capability}:${scope.purpose}`,scope])).values()];
 const boundaries=unique.map(scope=>{const key=`commerce-file-purchase:${randomUUID()}`;return {key,boundary:purchaseCommandBoundary(database,mode,key,scope)!};});
 const guards=(phase:string)=>unique.flatMap(scope=>{
  const key=`commerce-file-${phase}:${randomUUID()}`,actor=scope.actor,record=scope.source;
  const allowed=scope.permissions.map(permission=>assignmentReadSql("commerce_file_scope",permission)).join(" OR ");
  return [database.prepare("SELECT id FROM commerce_orders WHERE id=? AND company_id=? FOR SHARE").bind(record.id,record.company_id),database.prepare(`SELECT id FROM ${table(scope.target)} WHERE id=? FOR SHARE`).bind(scope.target.id),database.prepare("SELECT id FROM assignments WHERE id=? AND company_id=? FOR SHARE").bind(actor.assignmentId,actor.companyId),...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),
   assertion(database,key,"commerce-file-assignment",`(SELECT (${allowed}) FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) commerce_file_scope)`,[...scope.permissions.flatMap(()=>[actor.assignmentId,actor.userId,actor.role]),record.company_id,record.branch_id,record.department_id]),
   assertion(database,key,"commerce-file-source",`${image(scope.target)}=?::jsonb AND EXISTS(SELECT 1 FROM commerce_orders WHERE id=? AND company_id=? AND kind=? AND branch_id IS NOT DISTINCT FROM ?::text AND department_id IS NOT DISTINCT FROM ?::text)`,[scope.target.id,scope.witness,record.id,record.company_id,record.kind,record.branch_id,record.department_id]),
   database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)];
 });
 return {before:[...boundaries.flatMap(item=>item.boundary.before),...guards("before")],after:[...financeDeferredEffectStatements(database),...guards("after"),...boundaries.flatMap(item=>item.boundary.after),...boundaries.map(item=>database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key))],observe:mode==="shadow"?observePurchaseResults:undefined};
}
export function commerceAttachmentFailure(error:unknown){
 let current=error;
 for(let depth=0;depth<5&&current&&typeof current==="object";depth++){
  const failure=current as {code?:string;detail?:string;cause?:unknown};
  if(failure.code==="23514"&&failure.detail?.includes("commerce-file-"))return new ApiError(409,failure.detail.includes("commerce-file-source")?"COMMERCE_ATTACHMENT_SOURCE_CHANGED":"COMMERCE_ATTACHMENT_AUTHORITY_CHANGED","Commerce file authority or its actual source changed. Refresh and review before retrying.");
  current=failure.cause;
 }
 return null;
}
