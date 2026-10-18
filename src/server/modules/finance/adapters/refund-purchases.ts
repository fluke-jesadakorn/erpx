import "server-only";
import type {WorkContext} from "../../identity_access/public/authorization";
import {holdingResources} from "../../runtime/public/runtime";
import {isTransactionalSqlDatabase,type SqlDatabase} from "../../runtime/public/runtime/types";
import {ApiError} from "../../runtime/public/http";
import {financePackageEnforcementMode,readPurchasedAccesses} from "../../commercial/public/purchase-command-boundary";
import {requireFinanceDocumentHistory,withFinanceSourceDocuments,financeSourceDocumentReadLocks,financeSourceDocumentReadLeases} from "./document-purchases";
import {companyPurchaseReadLocks} from "../../commercial/public/purchase-command-boundary";
import {serviceRefundCashOwnerShape,serviceRefundCashSourceReadLocks,readServiceRefundCashSource,serviceRefundCashSourceGuard,type ServiceRefundCashSource} from "../../commerce/public/finance-refund-source";
import {requireFinanceCurrentAuthority} from "./purchase-boundary";
import {flushFinanceDeferredEffects} from "./finance-transactions";

type Target={type:"obligation";id:string}|{type:"instructions";ids:readonly string[]};
type Operation="finance.refund.approve"|"finance.refund.prepare"|"finance.refund.export"|"finance.refund.exception"|"finance.refund.confirm"|"finance.bank.import";
async function source(database:SqlDatabase,actor:WorkContext,target:Target,lock=false){
 if(target.type==="obligation")return {ids:[target.id],links:[] as {id:string;obligation_id:string}[]};
 const ids=[...new Set(target.ids)].sort();if(!ids.length)return {ids:[],links:[] as {id:string;obligation_id:string}[]};
 const links=(await database.prepare(`SELECT id,obligation_id FROM finance_refund_instructions WHERE company_id=? AND id IN (${ids.map(()=>"?").join(',')}) ORDER BY id${lock?" FOR SHARE":""}`).bind(actor.companyId,...ids).all<{id:string;obligation_id:string}>()).results;
 if(links.length!==ids.length)throw new ApiError(404,"REFUND_INSTRUCTION_NOT_FOUND","The actual refund instruction is unavailable in this Company.");
 return {ids:[...new Set(links.map(row=>row.obligation_id))].sort(),links};
}
export async function withRefundPurchasedRead<Result>(actor:WorkContext,target:Target,run:(database:SqlDatabase)=>Promise<Result>,recoverExport=false){
 const database=(await holdingResources(actor.holdingId)).write;
 if(await financePackageEnforcementMode()==="disabled")return run(database);
 let resolved=await source(database,actor,target);
 if(recoverExport&&target.type==="instructions"&&target.ids.length===1){
  const linked=(await database.prepare("SELECT sibling.id FROM finance_refund_instructions current JOIN finance_refund_instructions sibling ON sibling.export_command_id=current.export_command_id AND sibling.company_id=current.company_id WHERE current.id=? AND current.company_id=? ORDER BY sibling.id").bind(target.ids[0],actor.companyId).all<{id:string}>()).results;
  if(linked.length)resolved=await source(database,actor,{type:"instructions",ids:linked.map(row=>row.id)});
 }
 for(const id of resolved.ids)await requireFinanceDocumentHistory(database,actor,"refund",id);
 return run(database);
}
/** Original source-return, independent approval, Treasury, exact bank/version,
 * receipts and money effects remain in the normal Refund application. */
export async function withRefundPurchasedCommand<Result>(actor:WorkContext,target:Target,command:{key:string;operation:Operation;strictPurchase?:true},run:(database?:SqlDatabase)=>Promise<Result>,owned?:SqlDatabase){
 const disabled=!command.strictPurchase&&await financePackageEnforcementMode()==="disabled";
 if(disabled&&!owned)return run();
 const owner=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,"TRANSACTION_REQUIRED","Refund settlement requires the original Holding transaction.");
 if(owned){const ownership=owner as SqlDatabase&{assertActiveTransaction?:(database:SqlDatabase)=>void};if(!ownership.assertActiveTransaction)throw new ApiError(503,"TRANSACTION_OWNERSHIP_REQUIRED","Refund settlement requires its original owned Holding transaction.");ownership.assertActiveTransaction(owned);}
 if(disabled)return run(owned);
 const execute=async(transaction:SqlDatabase)=>{
  await transaction.batch(financeSourceDocumentReadLocks(transaction,actor.companyId));
  const original=await source(transaction,actor,target,true);
  const result=await withFinanceSourceDocuments(actor,original.ids.map(id=>({kind:"refund" as const,id})),command,run,transaction);
  const current=await source(transaction,actor,target);
  if(JSON.stringify(original)!==JSON.stringify(current))throw new ApiError(409,"FINANCE_REFUND_SOURCE_CHANGED","The actual refund instruction source changed before settlement committed.");
  return result;
 };
 return owned?execute(owned):owner.transaction(execute);
}
export type ServiceRefundCashPreparationScope={capture(obligationId:string,cashAccountId:string):Promise<ServiceRefundCashSource>};
const cashSourceChanged=()=>new ApiError(409,"FINANCE_REFUND_CASH_SOURCE_CHANGED","The actual Service receipt or refund cash source changed. Refresh and review before preparing the instruction.");
/** Only the original warehouse-free Service shape enters this native parent.
 * Ordinary legacy Refund commands retain their original constructor. */
export async function withServiceRefundCashPreparation<Result>(actor:WorkContext,id:string,key:string,run:(database?:SqlDatabase,cashScope?:ServiceRefundCashPreparationScope)=>Promise<Result>){
 const owner=(await holdingResources(actor.holdingId)).write,command={key,operation:"finance.refund.prepare" as const};
 if(!await serviceRefundCashOwnerShape(owner,actor,id))return withRefundPurchasedCommand(actor,{type:"obligation",id},command,run);
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,"TRANSACTION_REQUIRED","Service refund cash preparation requires the original Holding transaction.");
 try{return await owner.transaction(async database=>{
  // All source table leases precede the first native snapshot/advisory SELECT.
  // The closed writer option takes instruction SRE directly, without upgrade.
  await database.batch([...financeSourceDocumentReadLeases(database,actor.companyId,false,undefined,true),...serviceRefundCashSourceReadLocks(database),
   database.prepare("LOCK TABLE finance_parties,finance_charts,finance_accounts,finance_payments,finance_invoices,finance_journals,finance_journal_lines,finance_financial_events IN SHARE MODE"),
   database.prepare("LOCK TABLE finance_refund_obligations,money_transaction_snapshots,command_receipts IN SHARE ROW EXCLUSIVE MODE")]);
  await database.batch(companyPurchaseReadLocks(database,actor.companyId));
  if(!await serviceRefundCashOwnerShape(database,actor,id))throw cashSourceChanged();
  const captured=new Map<string,ServiceRefundCashSource>(),guard=`service-refund-cash:${crypto.randomUUID()}`;
  const cashScope:ServiceRefundCashPreparationScope={capture:async(obligationId,cashAccountId)=>{
   if(obligationId!==id)throw cashSourceChanged();
   await requireFinanceCurrentAuthority(database,actor,guard);
   const selected=await readServiceRefundCashSource(database,actor,id,cashAccountId);
   const first=captured.get(cashAccountId);
   if(first&&(!selected||first.image!==selected.image))throw cashSourceChanged();
   if(!selected)throw new ApiError(409,"CONFLICT","The record changed, a reference is unavailable, or this value already exists. Refresh and review again.");
   if(!first)captured.set(cashAccountId,selected);return first??selected;
  }};
  return withRefundPurchasedCommand(actor,{type:"obligation",id},command,async transaction=>{
   const result=await run(transaction,cashScope);
   await flushFinanceDeferredEffects(database);
   // No fallback/reselection after effects; the first actual receipt image wins.
   for(const selected of captured.values())await database.batch([serviceRefundCashSourceGuard(database,actor,guard,selected),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(guard)]);
   await requireFinanceCurrentAuthority(database,actor,guard);
   return result;
  },database);
 });}catch(error){if((error as {code?:string}).code==="23514"&&String((error as {detail?:string}).detail).includes("service-refund-cash-source"))throw cashSourceChanged();throw error;}
}
export async function refundSourcePurchase(database:SqlDatabase,actor:WorkContext,id:string){
 const mode=await financePackageEnforcementMode();if(mode==="disabled")return undefined;
 const source=await requireFinanceDocumentHistory(database,actor,"refund",id),access=(await readPurchasedAccesses(database,mode,actor.companyId,[source.capability]))[source.capability];
 return {capability:source.capability,settleExisting:access.settleExisting,history:access.history,revision:access.revision};
}
