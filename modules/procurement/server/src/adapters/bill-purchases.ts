import "server-only";
import {receiptImage} from './read-tail-r19';
import {sourceImage} from './read-tail-r31';
import {randomUUID} from "node:crypto";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import {iamMutationGuards} from "@erpx/identity-access-server/public/iam-store";
import {workContextGuards,financialConstraintError} from '@erpx/workflow-server/public/command-guards';
import {holdingResources} from "../../../../../platform/runtime/server/src/public/runtime";
import {isTransactionalSqlDatabase,type SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {flushFinanceDeferredEffects} from "@erpx/finance-server/public/finance-transactions";
import {commercePackageEnforcementMode,checkPurchasedCapability,readPurchasedAccess,companyPurchaseReadLocks,purchaseCommandBoundary,purchaseCommandFailure} from "@erpx/commercial-server/public/purchase-command-boundary";
import type {CapabilityPurpose} from "@erpx/commercial-server/public/rules/packages";
import {supplierBillSchema,requestedBillReceipts} from "@erpx/procurement-server/domain/supplier-bills";
import {clearWorkflowCommandAssertions} from "@erpx/workflow-server/public/command-assertion-effects";
import {commandReplayGuard,branchDepartmentScope} from './read-tail-sql';
import {assignmentScopeProbe} from './read-tail-r30';
import {billReceiptBinding} from './read-tail-r23';
import {foreignBillReceiptWitness} from './read-tail-r25';

const capability="supply.purchasing",foundations=[capability,"supply.receiving","finance.inventory-valuation"] as const;
export type SupplierBillPurchase={newWork:boolean;settleExisting:boolean;history:boolean;revision:string};
async function authority(database:SqlDatabase,actor:WorkContext,permissions:readonly string[],key:string){
 for(const permission of permissions){const row=await database.prepare(assignmentScopeProbe("bill_scope",permission)).bind(actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId).first<{allowed:boolean}>();if(!row?.allowed)throw new ApiError(403,"SUPPLIER_BILL_ACCESS_CHANGED","Choose a current authorized supplier-invoice work role.");}
 await database.batch([...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),clearWorkflowCommandAssertions(database, key)]);
}
export async function supplierBillPurchaseAccess(database:SqlDatabase,actor:WorkContext){const mode=await commercePackageEnforcementMode();if(mode==="disabled")return undefined;await authority(database,actor,["finance.read"],`bill-read:${randomUUID()}`);return readPurchasedAccess(database,mode,{companyId:actor.companyId,capability});}
export async function withSupplierBillPurchasedRead<Result>(actor:WorkContext,run:(database?:SqlDatabase,purchase?:SupplierBillPurchase)=>Promise<Result>,provided?:SqlDatabase,id?:string){const mode=await commercePackageEnforcementMode();if(mode==="disabled")return run(provided);const database=provided??(await holdingResources(actor.holdingId)).write;await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability,purpose:"history"});const access=await supplierBillPurchaseAccess(database,actor);if(id&&!await image(database,actor,id))throw new ApiError(404,"SUPPLIER_BILL_NOT_FOUND","The actual supplier invoice source was not found.");return run(database,access?{newWork:access.newWork,settleExisting:access.settleExisting,history:access.history,revision:access.revision}:undefined);}
export {supplierBillOwnerPredicate} from "@erpx/procurement-server/adapters/bill-source-sql";
const scoped=branchDepartmentScope("m");
async function image(database:SqlDatabase,actor:WorkContext,id:string){
 return database.prepare(sourceImage(`jsonb_build_object('bill',to_jsonb(m),'receipts',${receiptImage("m.id")})::text image`,`procurement_bill_matches m JOIN commerce_fulfillments primary_receipt ON primary_receipt.id=m.fulfillment_id AND primary_receipt.order_id=m.order_id JOIN commerce_orders primary_order ON primary_order.id=m.order_id AND primary_order.company_id=m.company_id AND primary_order.kind='purchase'`,`m.id=? AND ${scoped} AND ${billReceiptBinding(false,"br","m")} AND ${foreignBillReceiptWitness("m")}`)).bind(id,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).first<string>("image");
}
/** Bill matching discharges actual received Purchase obligations. It cannot
 * authorize a standalone AP invoice or a Sales/foreign receipt by its label. */
export async function withSupplierBillPurchasedCommand<Result>(actor:WorkContext,command:{key:string;operation:string;id:string;purpose:CapabilityPurpose;capture?:unknown;policy?:boolean;permissions:readonly string[]},run:(database:SqlDatabase)=>Promise<Result>){
 const owner=(await holdingResources(actor.holdingId)).write,mode=await commercePackageEnforcementMode();if(mode==="disabled")return run(owner);if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,"TRANSACTION_REQUIRED","Supplier invoice matching requires its original Holding transaction.");
 try{return await owner.transaction(async database=>{
  const locks=companyPurchaseReadLocks(database,actor.companyId);await database.batch([locks[0],database.prepare("LOCK TABLE commerce_orders,commerce_fulfillments,procurement_bill_matches,procurement_bill_match_receipts IN SHARE MODE"),...locks.slice(1)]);
  const replay=await database.prepare(commandReplayGuard).bind(command.key,command.operation,actor.userId,actor.companyId).first(),purpose=replay?"history" as const:command.purpose,key=`bill-purchase:${randomUUID()}`,boundaries=foundations.map((capability,index)=>({key:`${key}:${index}`,scope:{companyId:actor.companyId,capability,purpose}}));
  for(const item of boundaries)await checkPurchasedCapability(database,mode,item.scope);await authority(database,actor,command.permissions,key);
  if(command.capture&&!replay)for(const receipt of requestedBillReceipts(supplierBillSchema.parse(command.capture))){const source=await database.prepare(`SELECT o.id FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id WHERE f.id=? AND o.kind='purchase' AND ${branchDepartmentScope("o")} FOR SHARE OF f,o`).bind(receipt.fulfillmentId,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).first();if(!source)throw new ApiError(404,"RECEIPT_NOT_FOUND","The actual Purchase receipt is not available in this workspace.");}
  else if(!command.policy&&!await image(database,actor,command.id))throw new ApiError(404,"SUPPLIER_BILL_NOT_FOUND","The actual supplier invoice source was not found.");
  const purchases=boundaries.map(item=>({...item,boundary:purchaseCommandBoundary(database,mode,item.key,item.scope)!}));await database.batch(purchases.flatMap(item=>item.boundary.before));
  const result=await run(database),witness=command.policy?null:await image(database,actor,command.id);if(!command.policy&&!witness)throw new ApiError(409,"SUPPLIER_BILL_SOURCE_CHANGED","The actual Purchase invoice source changed.");
  await flushFinanceDeferredEffects(database);await authority(database,actor,command.permissions,key);
  if(witness){const after=await image(database,actor,command.id);if(after!==witness)throw new ApiError(409,"SUPPLIER_BILL_SOURCE_CHANGED","The actual Purchase invoice source changed before commit.");}
  const observed=await database.batch(purchases.flatMap(item=>[...item.boundary.after,clearWorkflowCommandAssertions(database, item.key)]));purchases[0]?.boundary.observe?.(observed);return result;
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
