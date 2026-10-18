import "server-only";
import {randomUUID} from "node:crypto";
import {hasPermission} from "@/lib/identity_access/permissions";
import type {WorkContext} from "../../identity_access/public/authorization";
import {assignmentReadSql} from "../../identity_access/public/record-source-authority";
import {iamMutationGuards} from "../../identity_access/public/iam-store";
import {workContextGuards,financialConstraintError} from "../../workflow/public/commands";
import {commercePackageEnforcementMode,readPurchasedAccesses,checkPurchasedCapability,companyPurchaseReadLocks,purchaseCommandBoundary,purchaseCommandFailure} from "../../commercial/public/purchase-command-boundary";
import type {CapabilityPurpose} from "../../commercial/public/rules/packages";
import {financeLandedCostReceiptSource} from "../../finance/public/landed-cost-receipt-purchases";
import {flushFinanceDeferredEffects} from "../../finance/public/finance-transactions";
import {isTransactionalSqlDatabase,type SqlDatabase} from "../../runtime/public/runtime/types";
import {holdingResources} from "../../runtime/public/runtime";
import {ApiError} from "../../runtime/public/http";
import {commerceLandedCostReceiptSources,commerceLandedCostReceiptSource,commerceLandedCostSourceReadLocks} from "../../commerce/public/landed-cost-purchase-sources";
const capabilities=["supply.advanced-inventory","supply.receiving","supply.stock"] as const;
export type LandedCostPurchaseControls={newWork:boolean;settleExisting:boolean;history:boolean;revision:string};
export const landedCostPurchaseSourceRows=`SELECT c.id,c.company_id,c.branch_id,receipt.warehouse_id,c.fulfillment_id,receipt.order_id,receipt.warehouse_active,
 jsonb_build_object('cost',to_jsonb(c)-'version'-'updated_at'-'status'-'approved_by'-'review_reason'-'journal_id','receipt',receipt.witness,'lines',COALESCE((SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM inventory_landed_cost_lines line WHERE line.landed_cost_id=c.id),'[]'::jsonb),'lots',COALESCE((SELECT jsonb_agg(to_jsonb(lot) ORDER BY lot.id) FROM inventory_landed_cost_lots lot JOIN inventory_landed_cost_lines line ON line.id=lot.landed_cost_line_id WHERE line.landed_cost_id=c.id),'[]'::jsonb)) witness
 FROM inventory_landed_costs c JOIN (${commerceLandedCostReceiptSources}) receipt ON receipt.id=c.fulfillment_id AND receipt.company_id=c.company_id AND receipt.branch_id=c.branch_id`;
export function landedCostSourceReadLocks(database:SqlDatabase){return [...commerceLandedCostSourceReadLocks(database),database.prepare("LOCK TABLE inventory_landed_costs,inventory_landed_cost_lines,inventory_landed_cost_lots,inventory_landed_cost_events IN SHARE MODE")];}
async function current(database:SqlDatabase,actor:WorkContext,key:string,permission="finance.read"){
 if(actor.departmentId||!hasPermission(actor,permission)||!hasPermission(actor,"finance.read"))throw new ApiError(403,"LANDED_COST_ACCESS_CHANGED","Choose a current original Finance assignment for landed cost.");
 for(const required of new Set(["finance.read",permission])){
 const row=await database.prepare(`SELECT ${assignmentReadSql("landed_cost_scope",required)} allowed FROM (SELECT ?::text company_id,?::text branch_id,NULL::text department_id) landed_cost_scope`).bind(actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId).first<{allowed:boolean}>();if(!row?.allowed)throw new ApiError(403,"LANDED_COST_ACCESS_CHANGED","The original landed-cost assignment is no longer current.");}
 await database.batch([...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]);
}
async function source(database:SqlDatabase,actor:WorkContext,id:string){const row=await database.prepare(`SELECT id,fulfillment_id,witness::text witness FROM (${landedCostPurchaseSourceRows}) actual_cost WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?)`).bind(id,actor.companyId,actor.branchId,actor.branchId).first<{id:string;fulfillment_id:string;witness:string}>();if(!row)throw new ApiError(404,"LANDED_COST_NOT_FOUND","The original landed-cost receipt is unavailable in this workspace.");return row;}
export async function landedCostSourceAccess(database:SqlDatabase,actor:WorkContext,id?:string):Promise<LandedCostPurchaseControls|undefined>{
 const mode=await commercePackageEnforcementMode();if(mode==="disabled")return undefined;await current(database,actor,`landed-cost-read:${randomUUID()}`);if(id)await source(database,actor,id);
 const access=await readPurchasedAccesses(database,mode,actor.companyId,capabilities);return {newWork:capabilities.every(capability=>access[capability].newWork),settleExisting:capabilities.every(capability=>access[capability].settleExisting),history:capabilities.every(capability=>access[capability].history),revision:access["supply.advanced-inventory"].revision};
}
export async function landedCostPurchaseControls(actor:WorkContext,id?:string){return landedCostSourceAccess((await holdingResources(actor.holdingId)).write,actor,id);}
export async function requireLandedCostPurchasedRead(database:SqlDatabase,actor:WorkContext,id?:string){const access=await landedCostSourceAccess(database,actor,id);if(access&&!access.history)throw new ApiError(403,"PACKAGE_REQUIRED","The original advanced inventory addon or retained history is required.");return access;}
/** Only the full landed-cost owner composes these guards. Original captured
 * quantities/lots, movement policy and independent Finance stay unchanged. */
export async function withLandedCostPurchasedCommand<Result>(owner:SqlDatabase,actor:WorkContext,command:{key:string;operation:string;id:string;purpose:CapabilityPurpose;permission:string;fulfillmentId?:string;financial?:boolean;existing?:boolean},run:(database:SqlDatabase)=>Promise<Result>){
 const mode=await commercePackageEnforcementMode();if(mode==="disabled")return run(owner);if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,"LANDED_COST_TRANSACTION_REQUIRED","Landed costs require their original owned Holding transaction.");
 try{return await owner.transaction(async database=>{
 const locks=companyPurchaseReadLocks(database,actor.companyId);await database.batch([locks[0],...landedCostSourceReadLocks(database),...locks.slice(1)]);const key=`landed-cost-purchase:${randomUUID()}`;await current(database,actor,key,command.permission);
 const replay=await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(command.key,command.operation,command.id,actor.userId,actor.companyId).first(),purpose=replay?"history" as const:command.purpose,before=command.existing||replay?await source(database,actor,command.id):null;
 const receiptId=before?.fulfillment_id??command.fulfillmentId;if(!receiptId)throw new ApiError(404,"LANDED_COST_NOT_FOUND","The original landed-cost receipt is unavailable.");
 const receipt=await commerceLandedCostReceiptSource(database,actor,receiptId);if(!receipt)throw new ApiError(409,"LANDED_COST_SOURCE_CHANGED","The original purchase receipt or warehouse policy is unavailable.");const finance=await financeLandedCostReceiptSource(database,{companyId:actor.companyId,receiptId,journalId:receipt.journal_id});if(!finance||finance.status!=="posted")throw new ApiError(409,"LANDED_COST_SOURCE_CHANGED","The original receipt journal must remain posted in its legal Company.");if(purpose==="new-work"&&!receipt.warehouse_active)throw new ApiError(409,"LANDED_COST_SOURCE_CHANGED","The receiving warehouse is no longer active for new allocation.");
 const scopes=[...capabilities,...(command.financial?["finance.inventory-valuation"]:[])].map(capability=>({companyId:actor.companyId,capability,purpose})),boundaries=scopes.map((scope,index)=>({key:`${key}:${index}`,boundary:purchaseCommandBoundary(database,mode,`${key}:${index}`,scope)!}));
 for(const scope of scopes)await checkPurchasedCapability(database,mode,scope);await current(database,actor,key,command.permission);await database.batch(boundaries.flatMap(item=>item.boundary.before));const result=await run(database);await flushFinanceDeferredEffects(database);
 const after=await source(database,actor,command.id),afterReceipt=await commerceLandedCostReceiptSource(database,actor,receiptId),afterFinance=afterReceipt?await financeLandedCostReceiptSource(database,{companyId:actor.companyId,receiptId,journalId:afterReceipt.journal_id}):null;if(!afterFinance||JSON.stringify(afterFinance)!==JSON.stringify(finance)||after.fulfillment_id!==receiptId||before&&after.witness!==before.witness||!afterReceipt||afterReceipt.witness!==receipt.witness||purpose==="new-work"&&!afterReceipt.warehouse_active)throw new ApiError(409,"LANDED_COST_SOURCE_CHANGED","The original landed-cost receipt, allocation or warehouse changed before commit.");
 await current(database,actor,key,command.permission);const observed=await database.batch(boundaries.flatMap(item=>[...item.boundary.after,database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key)]));boundaries[0]?.boundary.observe?.(observed);return result;
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
