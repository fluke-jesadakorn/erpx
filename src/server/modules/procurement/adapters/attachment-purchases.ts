import "server-only";
import {randomUUID} from "node:crypto";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import type {WorkContext} from "../../identity_access/public/authorization";
import type {AttachmentOperation,AttachmentPolicy} from "../../files/public/attachments/types";
import type {CapabilityPurpose} from "../../commercial/public/rules/packages";
import {commercePackageEnforcementMode,checkPurchasedCapability,purchaseCommandBoundary,observePurchaseResults,companyPurchaseReadLocks} from "../../commercial/public/purchase-command-boundary";
import {assignmentReadSql} from "../../identity_access/public/record-source-authority";
import {iamMutationGuards} from "../../identity_access/public/iam-store";
import {assertion,workContextGuards} from "../../workflow/public/commands";
import {financeDeferredEffectStatements} from "../../finance/public/finance-transactions";
import {ApiError} from "../../runtime/public/http";
import {procurementSourcePredicate} from "./record-purchases";
import {supplierBillSourcePredicate} from "./record-purchases";
import {supplierBillOwnerPredicate} from "./bill-source-sql";

export type ProcurementAttachmentPurchase={companyId:string;capability:"supply.purchasing";purpose:CapabilityPurpose;actor:WorkContext;targetId:string;targetType:"procurement_rfq"|"supplier_bill_match";witness:string};
const image="SELECT (to_jsonb(r)-'version'-'updated_at')::text image FROM procurement_rfqs r WHERE id=? AND company_id=?";
const billImage=()=>`SELECT jsonb_build_object('bill',to_jsonb(r)-'version'-'updated_at','receipts',(SELECT jsonb_agg(jsonb_build_object('binding',to_jsonb(br),'receipt',to_jsonb(f),'order',to_jsonb(o)) ORDER BY br.id) FROM procurement_bill_match_receipts br JOIN commerce_fulfillments f ON f.id=br.fulfillment_id JOIN commerce_orders o ON o.id=f.order_id WHERE br.match_id=r.id))::text image FROM procurement_bill_matches r WHERE r.id=? AND r.company_id=? AND ${supplierBillOwnerPredicate("r")}`;
/** The actual Files transaction takes these leases before its first source or
 * receipt snapshot; a pool preflight cannot hold a transaction source lease. */
export async function procurementAttachmentSourceReadLocks(database:SqlDatabase,scopes:readonly ProcurementAttachmentPurchase[]){
 if(!scopes.length||await commercePackageEnforcementMode()==="disabled")return [];
 const companies=[...new Set(scopes.map(scope=>scope.companyId))].sort().map(company=>companyPurchaseReadLocks(database,company));
 return [...companies.map(locks=>locks[0]),database.prepare("LOCK TABLE procurement_rfqs,commerce_orders,commerce_fulfillments,procurement_bill_matches,procurement_bill_match_receipts IN SHARE MODE"),...companies.flatMap(locks=>locks.slice(1))];
}
export async function procurementAttachmentPurchaseScope(database:SqlDatabase,actor:WorkContext,policy:Pick<AttachmentPolicy,"targetType"|"purposeKey">,id:string,purpose:CapabilityPurpose="new-work"):Promise<ProcurementAttachmentPurchase|null>{
 if(!["procurement_rfq","supplier_bill_match"].includes(policy.targetType)||await commercePackageEnforcementMode()==="disabled")return null;
 const targetType=policy.targetType as ProcurementAttachmentPurchase["targetType"],bill=targetType==="supplier_bill_match",witness=await database.prepare(bill?billImage():image).bind(id,actor.companyId).first<string>("image");if(!witness)throw new ApiError(404,"ATTACHMENT_TARGET_NOT_FOUND","The original Procurement source is unavailable.");
 return {companyId:actor.companyId,capability:"supply.purchasing",purpose:bill&&purpose==="new-work"?"settle-existing":purpose,actor,targetId:id,targetType,witness};
}
export async function authorizeProcurementAttachmentPurchase(database:SqlDatabase,actor:WorkContext,policy:AttachmentPolicy,id:string,operation:AttachmentOperation){
 const scope=await procurementAttachmentPurchaseScope(database,actor,policy,id,operation==="read"?"history":"new-work");if(!scope)return;
 const bill=scope.targetType==="supplier_bill_match",authority=await (bill?supplierBillSourcePredicate:procurementSourcePredicate)(database,actor,"r",false);
 if(!await database.prepare(`SELECT 1 FROM ${bill?"procurement_bill_matches":"procurement_rfqs"} r WHERE r.id=? AND ${authority.where}`).bind(id,...authority.args).first())throw new ApiError(404,"ATTACHMENT_TARGET_NOT_FOUND","The original Procurement source is unavailable.");
 await checkPurchasedCapability(database,await commercePackageEnforcementMode(),scope);
}
/** Files owns private bytes, Legal holds and its original policy/version. This
 * extra source/current-IAM fence runs after all original deferred effects. */
export async function procurementAttachmentCommandBoundary(database:SqlDatabase,scopes:readonly ProcurementAttachmentPurchase[]){
 const mode=await commercePackageEnforcementMode();if(mode==="disabled"||!scopes.length)return undefined;
 const unique=[...new Map(scopes.map(scope=>[`${scope.companyId}:${scope.targetType}:${scope.targetId}:${scope.purpose}`,scope])).values()],boundaries=unique.map(scope=>{const key=`rfq-file-purchase:${randomUUID()}`;return {key,boundary:purchaseCommandBoundary(database,mode,key,scope)!};});
 const guards=(phase:string)=>unique.flatMap(scope=>{const key=`rfq-file-${phase}:${randomUUID()}`,a=scope.actor,bill=scope.targetType==="supplier_bill_match",table=bill?"procurement_bill_matches":"procurement_rfqs",name=bill?"bill-file":"rfq-file";return [
  database.prepare(`SELECT id FROM ${table} WHERE id=? AND company_id=? FOR SHARE`).bind(scope.targetId,scope.companyId),database.prepare("SELECT id FROM assignments WHERE id=? AND company_id=? FOR SHARE").bind(a.assignmentId,a.companyId),...workContextGuards(database,a,key),...iamMutationGuards(database,a,key),
  assertion(database,key,`${name}-owner`,`(${bill?billImage():image})=?`,[scope.targetId,scope.companyId,scope.witness]),
  assertion(database,key,`${name}-authority`,`EXISTS(SELECT 1 FROM ${table} rfq_file_scope WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?) AND ${assignmentReadSql("rfq_file_scope",bill?"finance.write":"commerce.purchase")})`,[scope.targetId,scope.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId,a.assignmentId,a.userId,a.role]),
  database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)
 ];});
 return {before:[...boundaries.flatMap(item=>item.boundary.before),...guards("before")],after:[...financeDeferredEffectStatements(database),...guards("after"),...boundaries.flatMap(item=>item.boundary.after),...boundaries.map(item=>database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key))],observe:mode==="shadow"?observePurchaseResults:undefined};
}
export function procurementAttachmentFailure(error:unknown){
 let current=error;for(let depth=0;depth<5&&current&&typeof current==="object";depth++){const failure=current as {code?:string;detail?:string;cause?:unknown};if(failure.code==="23514"&&failure.detail?.includes("rfq-file-")){const result=new ApiError(409,"RFQ_ATTACHMENT_CHANGED","The RFQ file source or authority changed. Refresh before retrying.");result.cause=current;return result;}current=failure.cause;}return null;
}
