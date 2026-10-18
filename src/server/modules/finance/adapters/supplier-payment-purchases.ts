import "server-only";
import type {WorkContext} from "../../identity_access/public/authorization";
import {isTransactionalSqlDatabase,type SqlDatabase} from "../../runtime/public/runtime/types";
import {holdingResources} from "../../runtime/public/runtime";
import {withFinancePurchasedCommand,withFinancePurchasedRead,requireFinanceCurrentAuthority} from "./purchase-boundary";
import {financeSourceAccess} from "./source-purchases";
import {withFinanceSourceDocuments,financeSourceDocumentReadLocks,requireFinanceDocumentHistory} from "./document-purchases";
import {financePackageEnforcementMode,readPurchasedAccesses,checkPurchasedCapability} from "../../commercial/public/purchase-command-boundary";
import {commerceFinanceRecordSources} from "../../commerce/public/finance-record-sources";
import {ApiError} from "../../runtime/public/http";

import {supplierPaymentPresentation,type SupplierPaymentSourcePurchase} from "../domain/supplier-payment-purchase";
export type {SupplierPaymentSourcePurchase} from "../domain/supplier-payment-purchase";

type Operation=`finance.supplier_batch.${"create"|"approve"|"cancel"|"export"|"result"|"confirm"}`;
export type SupplierPaymentTarget={type:"new";id:string;invoiceIds:readonly string[]}|{type:"batch"|"instruction";id:string};
type ReadTarget=Exclude<SupplierPaymentTarget,{type:"new"}>|{type:"invoice";id:string};
export function supplierBatchOwnedSql(alias:string){return `EXISTS(SELECT 1 FROM finance_supplier_instructions owned_item WHERE owned_item.batch_id=${alias}.id) AND NOT EXISTS(SELECT 1 FROM finance_supplier_instructions owned_item WHERE owned_item.batch_id=${alias}.id AND NOT EXISTS(SELECT 1 FROM (${commerceFinanceRecordSources.purchase.invoice}) owned_source WHERE owned_source.id=owned_item.invoice_id AND owned_source.company_id=${alias}.company_id))`;}
export function supplierBatchSourceReadLocks(database:SqlDatabase){return [database.prepare("LOCK TABLE finance_supplier_batches,finance_supplier_instructions IN SHARE MODE")];}
async function source(database:SqlDatabase,actor:WorkContext,target:SupplierPaymentTarget){
 if(target.type==="new")return {id:target.id,invoiceIds:[...new Set(target.invoiceIds)].sort()};
 const batch=await database.prepare(target.type==="batch"?"SELECT id FROM finance_supplier_batches WHERE id=? AND company_id=?":"SELECT b.id FROM finance_supplier_batches b JOIN finance_supplier_instructions i ON i.batch_id=b.id WHERE i.id=? AND b.company_id=?").bind(target.id,actor.companyId).first<{id:string}>();
 if(!batch)throw new ApiError(404,"BATCH_NOT_FOUND","Supplier batch not found in this Company.");
 const invoiceIds=(await database.prepare("SELECT DISTINCT invoice_id FROM finance_supplier_instructions WHERE batch_id=? ORDER BY invoice_id").bind(batch.id).all<{invoice_id:string}>()).results.map(row=>row.invoice_id);if(!invoiceIds.length)throw new ApiError(409,"SUPPLIER_BATCH_SOURCE_REQUIRED","Supplier batches require their original invoice instructions.");return {id:batch.id,invoiceIds};
}
async function witness(database:SqlDatabase,companyId:string,id:string){return database.prepare("SELECT jsonb_build_object('batch',to_jsonb(b),'instructions',(SELECT jsonb_agg(to_jsonb(i) ORDER BY i.id) FROM finance_supplier_instructions i WHERE i.batch_id=b.id))::text image FROM finance_supplier_batches b WHERE b.id=? AND b.company_id=?").bind(id,companyId).first<string>("image");}
/** A full Banking buyer keeps its normal owner path. Included Supply can only
 * settle a whole actual Purchase-backed batch, including every export row. */
export async function withSupplierPaymentPurchaseCommand<Result>(actor:WorkContext,key:string,operation:Operation,run:(transaction?:SqlDatabase)=>Promise<Result>,target?:SupplierPaymentTarget,owned?:SqlDatabase){
 if(owned){const ownership=(await holdingResources(actor.holdingId)).write as SqlDatabase&{assertActiveTransaction?:(database:SqlDatabase)=>void};if(!ownership.assertActiveTransaction)throw new ApiError(503,"TRANSACTION_OWNERSHIP_REQUIRED","Supplier settlement requires the original Holding transaction.");ownership.assertActiveTransaction(owned);}
 const mode=await financePackageEnforcementMode();if(mode==="disabled"||!target)return withFinancePurchasedCommand(actor,{key,operation,purpose:"settle-existing",capability:"finance.banking"},run,owned);
 const owner=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,"TRANSACTION_REQUIRED","Supplier settlement requires its original Holding transaction.");
 const execute=async(transaction:SqlDatabase)=>{
  // No receipt, instruction or classification snapshot precedes these leases.
  const locks=financeSourceDocumentReadLocks(transaction,actor.companyId);await transaction.batch([...locks.slice(0,1),...supplierBatchSourceReadLocks(transaction),...locks.slice(1)]);
  const actual=await source(transaction,actor,target);let expected:string|null=null;
  const result=await withFinanceSourceDocuments(actor,actual.invoiceIds.map(id=>({kind:"invoice" as const,id})),{key,operation,baseCapability:"finance.banking"},async database=>{const result=await run(database);expected=await witness(transaction,actor.companyId,actual.id);if(!expected)throw new ApiError(409,"SUPPLIER_BATCH_SOURCE_CHANGED","The original supplier batch changed.");return result;},transaction);
  if(await witness(transaction,actor.companyId,actual.id)!==expected)throw new ApiError(409,"SUPPLIER_BATCH_SOURCE_CHANGED","Supplier instructions or source identity changed before commit.");return result;
 };
 if(owned)return execute(owned);
 return owner.transaction(execute);
}
export async function supplierPaymentSourceOptions(database:SqlDatabase,actor:WorkContext,target?:ReadTarget):Promise<{sourceOnly:boolean;purchase?:SupplierPaymentSourcePurchase}>{
 const mode=await financePackageEnforcementMode();if(mode==="disabled")return {sourceOnly:false};
 await requireFinanceCurrentAuthority(database,actor,`supplier-source-read:${crypto.randomUUID()}`);const access=await readPurchasedAccesses(database,mode,actor.companyId,["finance.banking","finance.inventory-valuation"]);
 const purchased=access["finance.inventory-valuation"];
 if(access["finance.banking"].history){
  if(target&&purchased.history){const ids=target.type==="invoice"?[target.id]:(await source(database,actor,target)).invoiceIds;let actual=ids.length>0;for(const id of ids)if(!await database.prepare(`SELECT 1 FROM (${commerceFinanceRecordSources.purchase.invoice}) owned WHERE id=? AND company_id=?`).bind(id,actor.companyId).first()){actual=false;break;}if(actual)return supplierPaymentPresentation(true,purchased,true);}
  return supplierPaymentPresentation(true,purchased,false);
 }
 await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:"finance.inventory-valuation",purpose:"history"});return supplierPaymentPresentation(false,purchased,!!target);
}
export async function supplierBatchSourceDocuments(database:SqlDatabase,actor:WorkContext,id:string){return (await source(database,actor,{type:"batch",id})).invoiceIds;}
export async function supplierBatchSourceWitness(database:SqlDatabase,actor:WorkContext,id:string){return witness(database,actor.companyId,id);}
export async function withSupplierPaymentPurchaseRead<Result>(actor:WorkContext,run:(database?:SqlDatabase)=>Promise<Result>,target?:ReadTarget){
 if(await financePackageEnforcementMode()==="disabled")return withFinancePurchasedRead(actor,run,"finance.banking");const database=(await holdingResources(actor.holdingId)).write,options=await supplierPaymentSourceOptions(database,actor);
 if(options.sourceOnly&&target){const ids=target.type==="invoice"?[target.id]:(await source(database,actor,target)).invoiceIds;for(const id of ids){const access=await requireFinanceDocumentHistory(database,actor,"invoice",id);if(access.capability!=="finance.inventory-valuation")throw new ApiError(403,"PACKAGE_REQUIRED","This supplier batch includes an invoice outside the purchased Supply source.");}}
 return run(database);
}
export async function supplierPaymentWorkAccess(actor:WorkContext){const database=(await holdingResources(actor.holdingId)).write,access=await financeSourceAccess(database,actor,["finance.banking","finance.inventory-valuation"]);return {newWork:access["finance.banking"].newWork||access["finance.inventory-valuation"].newWork,settleExisting:access["finance.banking"].settleExisting||access["finance.inventory-valuation"].settleExisting,history:access["finance.banking"].history||access["finance.inventory-valuation"].history};}
