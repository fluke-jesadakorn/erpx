import "server-only";
import type {SupplierInvoiceCorrections,SupplierPaymentCorrectionSource} from "@erpx/feature-finance/lib/supplier-payment-correction";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {isTransactionalSqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {ApiError} from "@erpx/server-runtime/public/http";
import {financePackageEnforcementMode,readPurchasedAccesses,checkPurchasedCapability} from "@erpx/server-commercial/public/purchase-command-boundary";
import {commerceFinanceRecordSources} from "@erpx/server-commerce/public/finance-record-sources";
import {financeSourceDocumentReadLocks,financeDocumentSourceGuards} from "@erpx/server-finance/adapters/document-purchases";
import {supplierPaymentOriginalSources,financePaymentReversalSources} from "@erpx/server-finance/adapters/supplier-payment-correction-sources";
import {requireFinanceCurrentAuthority,withFinancePurchasedCommand} from "@erpx/server-finance/adapters/purchase-boundary";
import {flushFinanceDeferredEffects} from "@erpx/server-finance/adapters/finance-transactions";
import {inFinanceSignatureParent} from '@erpx/server-finance/adapters/tax-signature-read-parent';
type Target={kind:"payment";id:string}|{kind:"create";invoiceId:string;correctionPaymentId:string};
type Operation=`finance.payment.${"create"|"approve"|"release"|"confirm"|"reverse"}`;
type Command={key:string;operation:Operation;bankTransactionId?:string};
type Source={id:string;invoice_id:string;bank_transaction_id:string;bank_account:string;currency:string;amount_minor:number;party_id:string;status:string;image:string};
async function source(database:SqlDatabase,actor:WorkContext,target:Target):Promise<Source|null>{
 const originalId=target.kind==="create"?target.correctionPaymentId:target.id;
 // This is only a necessary native shape probe, never source authority. A
 // controlled replacement has no original Supplier instruction of its own;
 // do not plan the full original/reversal proof for that impossible candidate.
 // Selected originals still run the complete exact proof below. The owned
 // command leases hold both relations; every final source read repeats this
 // probe and the actual receipt-root proof, without a cross-request cache.
 const originalCandidate=await database.prepare("SELECT 1 FROM finance_payments payment JOIN finance_supplier_instructions instruction ON instruction.payment_id=payment.id AND instruction.status='confirmed' WHERE payment.id=? AND payment.company_id=?").bind(originalId,actor.companyId).first();
 let value=originalCandidate?await database.prepare(`SELECT original.*,jsonb_build_array(to_jsonb(original),to_jsonb(invoice),to_jsonb(bank))::text image FROM (${supplierPaymentOriginalSources}) original JOIN (${commerceFinanceRecordSources.purchase.invoice}) owned ON owned.id=original.invoice_id AND owned.company_id=original.company_id JOIN finance_invoices invoice ON invoice.id=original.invoice_id JOIN finance_bank_transactions bank ON bank.id=original.bank_transaction_id AND bank.company_id=original.company_id WHERE original.id=? AND original.company_id=? AND (original.status='settled' OR EXISTS(SELECT 1 FROM (${financePaymentReversalSources}) reversal WHERE reversal.id=original.id AND reversal.company_id=original.company_id))`).bind(originalId,actor.companyId).first<Source>():null;
 if(value)return target.kind==="create"&&value.invoice_id!==target.invoiceId?null:value;
 if(target.kind!=="payment")return null;
 const created=await database.prepare("SELECT receipt.result_json::jsonb->>'correctionPaymentId' correction_id,receipt.result_json::jsonb->>'settlementInvoiceId' invoice_id FROM command_receipts receipt JOIN finance_payments payment ON payment.id=receipt.record_id AND payment.company_id=? AND receipt.actor_id=payment.created_by WHERE receipt.record_id=? AND receipt.operation='finance.payment.create' AND receipt.authority_json::jsonb->>'companyId'=?").bind(actor.companyId,target.id,actor.companyId).first<{correction_id:string|null;invoice_id:string|null}>();
 if(!created?.correction_id||!created.invoice_id)return null;
 value=await source(database,actor,{kind:"create",invoiceId:created.invoice_id,correctionPaymentId:created.correction_id});if(!value||value.status!=="voided")return null;
 const payment=await database.prepare("SELECT * FROM finance_payments WHERE id=? AND company_id=? AND party_id=? AND currency=? AND kind='payment' AND bank_account=? AND amount_minor=? AND NOT EXISTS(SELECT 1 FROM finance_allocations allocation WHERE allocation.payment_id=? AND (allocation.invoice_id<>? OR allocation.company_id<>?))").bind(target.id,actor.companyId,value.party_id,value.currency,value.bank_account,value.amount_minor,target.id,value.invoice_id,actor.companyId).first();
 return payment?{...value,image:JSON.stringify([value.image,payment])}:null;
}
const identity=(value:Source)=>JSON.stringify([value.id,value.invoice_id,value.bank_transaction_id,value.bank_account,value.currency,value.amount_minor,value.party_id]);
/** The original supplier instruction and bank observation are retained. Only
 * its real reversal can anchor the normal independent controlled correction. */
export async function withSupplierPaymentCorrection<Result>(actor:WorkContext,target:Target,command:Command,run:(database?:SqlDatabase)=>Promise<Result>,fallback:()=>Promise<Result>,owned?:SqlDatabase){
 if(owned){const ownership=(await holdingResources(actor.holdingId)).write as SqlDatabase&{assertActiveTransaction?:(database:SqlDatabase)=>void};if(!ownership.assertActiveTransaction)throw new ApiError(503,'TRANSACTION_OWNERSHIP_REQUIRED','Supplier correction requires the original Holding transaction.');ownership.assertActiveTransaction(owned);}
 const mode=await financePackageEnforcementMode();if(mode!=="enforced"&&target.kind!=="create")return owned?withFinancePurchasedCommand(actor,{key:command.key,operation:command.operation,purpose:"settle-existing",capability:"finance.banking"},run,owned):fallback();
 const owner=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,"TRANSACTION_REQUIRED","Supplier correction requires its original Holding transaction.");
 const execute=async(database:SqlDatabase)=>{
  await database.batch(financeSourceDocumentReadLocks(database,actor.companyId,true,"supplier-correction"));
  const receipt=await database.prepare("SELECT result_json FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(command.key,command.operation,actor.userId,actor.companyId).first<{result_json:string}>(),purpose=receipt?"history" as const:"settle-existing" as const;
  await requireFinanceCurrentAuthority(database,actor,`supplier-correction:${crypto.randomUUID()}`);
  const purchases=await readPurchasedAccesses(database,mode,actor.companyId,["finance.banking","finance.inventory-valuation"]);
  if(target.kind!=="create"&&purchases["finance.banking"].codes[purpose]==="GRANTED")return {handled:false as const};
  const before=await source(database,actor,target);
  if(!before){if(target.kind==="create")throw new ApiError(422,"SUPPLIER_CORRECTION_SOURCE_REQUIRED","Choose the original reversed supplier payment and its actual purchasing invoice.");return {handled:false as const};}
  if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_TREASURY_REQUIRED","Use the original Company Finance assignment for this supplier correction.");
  if(target.kind==="create"&&before.status!=="voided")throw new ApiError(409,"SUPPLIER_CORRECTION_SOURCE_REQUIRED","Reverse the original supplier payment through independent Finance review first.");
  const capability=purchases["finance.banking"].codes[purpose]==="GRANTED"?"finance.banking":"finance.inventory-valuation";
  await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability,purpose});
  if(command.bankTransactionId&&command.bankTransactionId!==before.bank_transaction_id)throw new ApiError(422,"SUPPLIER_CORRECTION_BANK_SOURCE_INVALID","Use the retained bank observation from this exact original supplier payment.");
  if(target.kind==="create"&&!receipt){
   const bank=await database.prepare("SELECT 1 FROM finance_bank_transactions WHERE id=? AND company_id=? AND currency=? AND amount_minor=? AND match_status IN ('unmatched','proposed')").bind(before.bank_transaction_id,actor.companyId,before.currency,-before.amount_minor).first();
   const pending=await database.prepare("SELECT 1 FROM command_receipts receipt JOIN finance_payments payment ON payment.id=receipt.record_id AND payment.company_id=? AND payment.status<>'voided' WHERE receipt.operation='finance.payment.create' AND receipt.authority_json::jsonb->>'companyId'=? AND receipt.result_json::jsonb->>'correctionPaymentId'=?").bind(actor.companyId,actor.companyId,before.id).first();
   if(!bank||pending)throw new ApiError(409,"SUPPLIER_CORRECTION_ALREADY_PREPARED","Review the existing controlled correction or the original bank observation before preparing another one.");
  }
  const result=await withFinancePurchasedCommand(actor,{key:command.key,operation:command.operation,purpose:"settle-existing",capability},run,database);
  await flushFinanceDeferredEffects(database);
  const after=await source(database,actor,target);if(!after||identity(after)!==identity(before))throw new ApiError(409,"SUPPLIER_PAYMENT_CORRECTION_SOURCE_CHANGED","The original supplier payment or purchasing source changed during this correction.");
  const guard=`supplier-correction-final:${crypto.randomUUID()}`;await requireFinanceCurrentAuthority(database,actor,guard);await database.batch([...financeDocumentSourceGuards(database,actor,"invoice",after.invoice_id,"finance.inventory-valuation",guard),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(guard)]);
  if(target.kind==="create"){
   const captured=await database.prepare("SELECT payment.* FROM command_receipts receipt JOIN finance_payments payment ON payment.id=receipt.record_id WHERE receipt.command_id=? AND receipt.operation='finance.payment.create' AND receipt.actor_id=? AND receipt.authority_json::jsonb->>'companyId'=? AND receipt.result_json::jsonb->>'correctionPaymentId'=? AND receipt.result_json::jsonb->>'settlementInvoiceId'=? AND payment.company_id=? AND payment.party_id=? AND payment.currency=? AND payment.bank_account=? AND payment.kind='payment' AND payment.amount_minor=?").bind(command.key,actor.userId,actor.companyId,before.id,before.invoice_id,actor.companyId,before.party_id,before.currency,before.bank_account,before.amount_minor).first();
   if(!captured)throw new ApiError(422,"SUPPLIER_CORRECTION_AMOUNT_MISMATCH","Retain the exact original supplier, bank account, currency and amount for this correction.");
  }
  if((await source(database,actor,target))?.image!==after.image)throw new ApiError(409,"SUPPLIER_PAYMENT_CORRECTION_SOURCE_CHANGED","The original supplier correction changed before commit.");
  return {handled:true as const,result};
 };
 if(owned){
  const resolved=await execute(owned);return resolved.handled?resolved.result:withFinancePurchasedCommand(actor,{key:command.key,operation:command.operation,purpose:'settle-existing',capability:'finance.banking'},run,owned);
 }
 const resolved=await(command.operation==='finance.payment.confirm'?inFinanceSignatureParent(owner,actor,{key:command.key,operation:'finance.payment.confirm',signatureSource:'supplier-correction'},execute):owner.transaction(execute));
 return resolved.handled?resolved.result:fallback();
}
export async function supplierPaymentCorrectionPurchase(database:SqlDatabase,actor:WorkContext,target:{kind:"payment";id:string}){
 const mode=await financePackageEnforcementMode();if(mode==="disabled")return undefined;const value=await source(database,actor,target);if(!value)return undefined;
 await requireFinanceCurrentAuthority(database,actor,`supplier-correction-read:${crypto.randomUUID()}`);const access=(await readPurchasedAccesses(database,mode,actor.companyId,["finance.inventory-valuation"]))["finance.inventory-valuation"];
 return {capability:"finance.inventory-valuation" as const,settleExisting:access.settleExisting,history:access.history,revision:access.revision};
}
/** Eligible anchors are separate real payments, even when several original
 * receipts produced the same consolidated invoice. Never choose one silently. */
export async function supplierInvoiceCorrections(actor:WorkContext,invoiceId:string):Promise<SupplierInvoiceCorrections|undefined>{
 const database=(await holdingResources(actor.holdingId)).write,mode=await financePackageEnforcementMode();if(mode==="disabled"||actor.branchId||actor.departmentId)return undefined;
 await requireFinanceCurrentAuthority(database,actor,`supplier-invoice-correction:${crypto.randomUUID()}`);
 const accesses=await readPurchasedAccesses(database,mode,actor.companyId,["finance.inventory-valuation","finance.banking"]),access=accesses["finance.inventory-valuation"];if(!access.history&&!accesses["finance.banking"].history)return undefined;
 const candidates=(await database.prepare(`SELECT DISTINCT original.id,original.amount_minor,original.bank_account,source_payment.payment_number,account.code||' · '||account.name bank_account_label,bank.external_id bank_reference,(account.status='active' AND bank.match_status IN ('unmatched','proposed') AND invoice.status IN ('posted','partially_paid') AND invoice.outstanding_minor>=original.amount_minor AND NOT EXISTS(SELECT 1 FROM command_receipts created JOIN finance_payments payment ON payment.id=created.record_id AND payment.status<>'voided' WHERE created.operation='finance.payment.create' AND created.authority_json::jsonb->>'companyId'=original.company_id AND created.result_json::jsonb->>'correctionPaymentId'=original.id)) eligible FROM (${supplierPaymentOriginalSources}) original JOIN (${financePaymentReversalSources}) reversal ON reversal.id=original.id AND reversal.company_id=original.company_id JOIN (${commerceFinanceRecordSources.purchase.invoice}) owner_invoice ON owner_invoice.id=original.invoice_id AND owner_invoice.company_id=original.company_id JOIN finance_bank_transactions bank ON bank.id=original.bank_transaction_id AND bank.company_id=original.company_id JOIN finance_invoices invoice ON invoice.id=original.invoice_id AND invoice.company_id=original.company_id JOIN finance_payments source_payment ON source_payment.id=original.id AND source_payment.company_id=original.company_id JOIN finance_accounts account ON account.id=original.bank_account AND account.company_id=original.company_id WHERE original.invoice_id=? AND original.company_id=? ORDER BY source_payment.payment_number,original.id`).bind(invoiceId,actor.companyId).all<{id:string;amount_minor:number;bank_account:string;payment_number:string;bank_account_label:string;bank_reference:string;eligible:boolean|null}>()).results;
 if(!candidates.length)return undefined;
 const sources:SupplierPaymentCorrectionSource[]=candidates.filter(row=>row.eligible===true).map(row=>({paymentId:row.id,paymentNumber:row.payment_number,amountMinor:Number(row.amount_minor),bankAccountId:row.bank_account,bankAccountLabel:row.bank_account_label,bankReference:row.bank_reference}));
 const prepared=(await database.prepare(`SELECT DISTINCT payment.id,payment.payment_number,payment.status,created.result_json::jsonb->>'correctionPaymentId' original_id FROM command_receipts created JOIN finance_payments payment ON payment.id=created.record_id AND payment.company_id=? AND payment.created_by=created.actor_id JOIN finance_payments original ON original.id=created.result_json::jsonb->>'correctionPaymentId' AND original.company_id=payment.company_id WHERE created.operation='finance.payment.create' AND created.result_json::jsonb->>'id'=payment.id AND created.authority_json::jsonb->>'companyId'=? AND created.result_json::jsonb->>'settlementInvoiceId'=? AND original.id IN (${candidates.map(()=>"?").join(",")}) AND payment.status<>'voided' AND payment.kind='payment' AND payment.party_id=original.party_id AND payment.currency=original.currency AND payment.bank_account=original.bank_account AND payment.amount_minor=original.amount_minor AND NOT EXISTS(SELECT 1 FROM finance_allocations allocation WHERE allocation.payment_id=payment.id AND (allocation.company_id<>payment.company_id OR allocation.invoice_id<>?)) ORDER BY payment.payment_number,payment.id`).bind(actor.companyId,actor.companyId,invoiceId,...candidates.map(row=>row.id),invoiceId).all<{id:string;payment_number:string;status:string;original_id:string}>()).results.map(row=>({paymentId:row.id,paymentNumber:row.payment_number,status:row.status,originalPaymentId:row.original_id}));
 return {capability:"finance.inventory-valuation",settleExisting:access.settleExisting,history:access.history,revision:access.revision,sources,prepared};
}
/** Compatibility for the original single-anchor presentation contract. */
export async function supplierInvoiceCorrectionControls(actor:WorkContext,invoiceId:string){
 const options=await supplierInvoiceCorrections(actor,invoiceId);if(options?.sources.length!==1)return undefined;const source=options.sources[0];return {capability:options.capability,settleExisting:options.settleExisting,history:options.history,revision:options.revision,correctionPaymentId:source.paymentId,amountMinor:source.amountMinor,bankAccountId:source.bankAccountId};
}

/** Internal Finance reader composition returns only the exact already-owned
 * invoice and retained bank IDs for its existing normal payment workspace. */
export async function supplierPaymentCorrectionSourceIds(database:SqlDatabase,actor:WorkContext,id:string){const value=await source(database,actor,{kind:"payment",id});return value?{invoiceId:value.invoice_id,bankTransactionId:value.bank_transaction_id}:null;}
