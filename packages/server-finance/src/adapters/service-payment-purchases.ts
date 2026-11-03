import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {holdingResources} from "@erpx/server-runtime/public/runtime";
import {ApiError} from "@erpx/server-runtime/public/http";
import {financePackageEnforcementMode,readPurchasedAccesses} from "@erpx/server-commercial/public/purchase-command-boundary";
import {fieldServiceFinanceRecordSources} from "@erpx/server-field_service/public/finance-record-sources";
import {requireFinanceCurrentAuthority,withFinancePurchasedCommand} from "@erpx/server-finance/adapters/purchase-boundary";
import type {CapabilityPurpose} from "@erpx/server-commercial/public/rules/packages";

type Target={kind:"invoice"|"payment";id:string};
type Command={key:string;operation:string;purpose:CapabilityPurpose};
async function serviceSource(database:SqlDatabase,actor:WorkContext,target:Target){
 return database.prepare(`SELECT id,${target.kind==="payment"?"invoice_id":"id invoice_id"} FROM (${fieldServiceFinanceRecordSources[target.kind]}) actual WHERE actual.id=? AND actual.company_id=?`).bind(target.id,actor.companyId).first<{id:string;invoice_id:string}>();
}
/** Remaining consumer: non-Field Finance banking commands. Field targets are
 * unavailable here; their complete command goes through native ports. Remove
 * this entry when the remaining banking owner migrates. It writes no Field data. */
export async function withLegacyPaymentCommand<Result>(actor:WorkContext,target:Target|undefined,command:Command,run:(database?:SqlDatabase)=>Promise<Result>){
 if(target){
  const database=(await holdingResources(actor.holdingId)).write;
  const query=target.kind==='invoice'
   ?"SELECT EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND source_type='field_service') marker,EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=?) scoped"
   :"SELECT erpx_finance_field_payment_owned(?) marker,EXISTS(SELECT 1 FROM finance_payments WHERE id=? AND company_id=?) scoped";
  const source=await database.prepare(query).bind(target.id,target.id,actor.companyId).first<{marker:boolean;scoped:boolean}>();
  if(!source||typeof source.marker!=='boolean'||typeof source.scoped!=='boolean')throw new ApiError(503,'PAYMENT_OWNER_UNAVAILABLE','Could not verify this payment.');
  if(source.marker){
   if(!source.scoped)throw new ApiError(404,'PAYMENT_NOT_FOUND','Payment not found in this company.');
   throw new ApiError(503,'NATIVE_SERVICE_PAYMENT_REQUIRED','Use the current payment workflow. Your draft is retained.');
  }
 }
 return withFinancePurchasedCommand(actor,{...command,capability:'finance.banking'},run);
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
