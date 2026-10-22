import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {isTransactionalSqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {ApiError} from "@erpx/server-runtime/public/http";
import {financePackageEnforcementMode,readPurchasedAccesses,checkPurchasedCapability} from "@erpx/server-commercial/public/purchase-command-boundary";
import {fieldServiceFinanceRecordSources} from "@erpx/server-field_service/public/finance-record-sources";
import {financeSourceDocumentReadLocks,financeDocumentSourceGuards} from "@erpx/server-finance/adapters/document-purchases";
import {requireFinanceCurrentAuthority,withFinancePurchasedCommand} from "@erpx/server-finance/adapters/purchase-boundary";
import type {CapabilityPurpose} from "@erpx/server-commercial/public/rules/packages";
import {inFinanceSignatureParent} from '@erpx/server-finance/adapters/tax-signature-read-parent';

type Target={kind:"invoice"|"payment";id:string};
type Command={key:string;operation:string;purpose:CapabilityPurpose;bankTransactionId?:string};
async function serviceSource(database:SqlDatabase,actor:WorkContext,target:Target){
 return database.prepare(`SELECT id,${target.kind==="payment"?"invoice_id":"id invoice_id"} FROM (${fieldServiceFinanceRecordSources[target.kind]}) actual WHERE actual.id=? AND actual.company_id=?`).bind(target.id,actor.companyId).first<{id:string;invoice_id:string}>();
}
/** Actual normal controlled payment commands keep their original application,
 * independent reviewer/Treasury/bank/FX/version/receipt rules. The included
 * Service foundation authorizes only the immutable released invoice source. */
export async function withServicePaymentCommand<Result>(actor:WorkContext,target:Target|undefined,command:Command,run:(database?:SqlDatabase)=>Promise<Result>){
 const mode=await financePackageEnforcementMode();
 if(mode!=="enforced"||!target)return withFinancePurchasedCommand(actor,{...command,capability:"finance.banking"},run);
 const owner=(await holdingResources(actor.holdingId)).write;
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,"TRANSACTION_REQUIRED","Controlled Service settlement requires its original Holding transaction.");
 const execute=async(database:SqlDatabase)=>{
  // Owner writer leases precede any advisory SELECT or source/receipt snapshot.
  // A SHARE reader followed by payment/allocation writes would upgrade and
  // deadlock across Companies. Keep the same Support-first order as Service.
  await database.batch(financeSourceDocumentReadLocks(database,actor.companyId,true));
  const receipt=await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(command.key,command.operation,actor.userId,actor.companyId).first();
  const purpose=receipt?"history" as const:command.purpose;
  await requireFinanceCurrentAuthority(database,actor,`service-payment:${crypto.randomUUID()}`);
  const access=await readPurchasedAccesses(database,mode,actor.companyId,["finance.banking","finance.service-invoice"]);
  if(access["finance.banking"].codes[purpose]==="GRANTED")return withFinancePurchasedCommand(actor,{...command,capability:"finance.banking"},run,database);
  const source=await serviceSource(database,actor,target);
  if(!source)return withFinancePurchasedCommand(actor,{...command,capability:"finance.banking"},run,database);
  await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:"finance.service-invoice",purpose});
  if(command.bankTransactionId&&!receipt){
   const evidence=await database.prepare(`SELECT 1 FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id AND s.company_id=t.company_id JOIN command_receipts r ON r.record_id=s.id AND r.operation='finance.bank.import' AND r.authority_json::jsonb->>'companyId'=s.company_id AND r.result_json::jsonb->>'settlementPaymentId'=? AND r.result_json::jsonb->>'id'=s.id WHERE t.id=? AND t.company_id=?`).bind(target.id,command.bankTransactionId,actor.companyId).first();
   if(!evidence)throw new ApiError(422,"SERVICE_PAYMENT_BANK_SOURCE_INVALID","Import the bank evidence for this exact controlled Service receipt.");
  }
  const guard=async()=>{
   const key=`service-payment-source:${crypto.randomUUID()}`;
   const current=await serviceSource(database,actor,target);
   if(!current||current.invoice_id!==source.invoice_id)throw new ApiError(409,"SERVICE_PAYMENT_SOURCE_CHANGED","The controlled payment's original Service invoice changed. Refresh its source workflow.");
   await database.batch([...financeDocumentSourceGuards(database,actor,target.kind,target.id,"finance.service-invoice",key),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]);
  };
  await guard();
  const result=await withFinancePurchasedCommand(actor,{...command,capability:"finance.service-invoice"},run,database);
  await guard();return result;
 };
 return command.operation==='finance.payment.confirm'?inFinanceSignatureParent(owner,actor,{key:command.key,operation:'finance.payment.confirm'},execute):owner.transaction(execute);
}
/** Minimal owner controls for one actual record, without grant/IAM snapshots. */
export async function servicePaymentPurchase(database:SqlDatabase,actor:WorkContext,target:Target){
 const mode=await financePackageEnforcementMode();if(mode==="disabled")return undefined;
 const source=await serviceSource(database,actor,target);if(!source)return undefined;
 await requireFinanceCurrentAuthority(database,actor,`service-payment-controls:${crypto.randomUUID()}`);
 const access=(await readPurchasedAccesses(database,mode,actor.companyId,["finance.service-invoice"]))["finance.service-invoice"];
 return {capability:"finance.service-invoice",settleExisting:access.settleExisting,history:access.history,revision:access.revision};
}

export async function servicePaymentControls(actor:WorkContext,target:Target){return servicePaymentPurchase((await holdingResources(actor.holdingId)).write,actor,target);}
