import 'server-only';
import {createHash} from 'node:crypto';
import {samePerson} from '@erpx/feature-identity_access/lib/iam';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {requirePermission} from '@erpx/server-identity_access/public/iam-permissions';
import {lockQueuedCommandAuthority,requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {isTransactionalSqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {ApiError} from '@erpx/server-runtime/public/http';
import {financePackageEnforcementMode,readPurchasedAccesses,companyPurchaseReadLocks,checkPurchasedCapability} from '@erpx/server-commercial/public/purchase-command-boundary';
import {commerceFinanceRecordSources} from '@erpx/server-commerce/public/finance-record-sources';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import { assertProductionModuleEnabled } from "@erpx/server-pms/public/feature-availability";
import {financeSourceDocumentReadLeases} from '@erpx/server-finance/adapters/document-purchases';
import {withFinancePurchasedCommand} from '@erpx/server-finance/adapters/purchase-boundary';
import {financeSalesReceiptOriginalSources} from '@erpx/server-finance/adapters/sales-receipt-sources';
import {financePaymentReversalSources} from '@erpx/server-finance/adapters/supplier-payment-correction-sources';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/adapters/finance-transactions';

type Source={invoice_id:string;image:string;status:string;created_by:string};
/** The original Treasury command, issued immutable receipt and sole actual
 * allocation must agree. Payment kind, amount or a source label alone grants
 * no included correction authority. This relationship survives its reversal. */
async function source(database:SqlDatabase,actor:WorkContext,id:string){
 return database.prepare(`SELECT original.invoice_id,original.status,original.created_by,original.image
 FROM (${financeSalesReceiptOriginalSources}) original JOIN (${commerceFinanceRecordSources.sales.invoice}) owned ON owned.id=original.invoice_id AND owned.company_id=original.company_id
 WHERE original.id=? AND original.company_id=? AND (original.status='settled' OR EXISTS(SELECT 1 FROM (${financePaymentReversalSources}) reversal WHERE reversal.id=original.id AND reversal.company_id=original.company_id))`)
 .bind(id,actor.companyId).first<Source>();
}
async function authority(database:SqlDatabase,actor:WorkContext){
 requirePermission(actor,'finance.read');requirePermission(actor,'finance.write');
 if(actor.scope.kind!=='holding'||actor.branchId||actor.departmentId)throw new ApiError(403,'COMPANY_TREASURY_REQUIRED','Use a current Company Finance assignment for this Sales receipt correction.');
 const current=await requireQueuedCommandAuthority(database,actor,'finance.write');requirePermission(current,'finance.read');
 if(!hasCorrectionAuthority(actor)||!hasCorrectionAuthority(current))throw new ApiError(403,'INDEPENDENT_CORRECTION_REQUIRED','A different finance approver must reverse the payment.');
 await assertProductionModuleEnabled(database,actor.companyId,'sales');await assertProductionModuleEnabled(database,actor.companyId,'finance');
 return JSON.stringify([current.userId,current.assignmentId,current.role,current.authority??null,current.iam]);
}
/** Closed reversal only; all original independent correction, period, balance,
 * mirror journals, books, FX, money and receipt behavior stays in Finance. */
export async function withSalesReceiptReversal<Result>(actor:WorkContext,paymentId:string,key:string,run:(database:SqlDatabase)=>Promise<Result>,fallback:()=>Promise<Result>){
 const mode=await financePackageEnforcementMode();if(mode!=='enforced')return fallback();
 const owner=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'TRANSACTION_REQUIRED','Sales receipt correction requires its owned Holding transaction.');
 const resolved=await owner.transaction(async database=>{
  await database.batch([...financeSourceDocumentReadLeases(database,actor.companyId,true,'supplier-correction'),database.prepare('LOCK TABLE finance_customer_receipts,finance_periods IN SHARE MODE')]);
  await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(companyPurchaseReadLocks(database,actor.companyId).slice(1));
  const receipt=await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation='finance.payment.reverse' AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(key,actor.userId,actor.companyId).first(),purpose=receipt?'history' as const:'settle-existing' as const;
  const access=await readPurchasedAccesses(database,mode,actor.companyId,['finance.banking','finance.sales-posting']);
  if(access['finance.banking'].codes[purpose]==='GRANTED')return {handled:false as const};
  const before=await source(database,actor,paymentId);if(!before)return {handled:false as const};
  const initialAuthority=await authority(database,actor);await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:'finance.sales-posting',purpose});
  const result=await withFinancePurchasedCommand(actor,{key,operation:'finance.payment.reverse',purpose:'settle-existing',capability:'finance.sales-posting'},database=>{if(!database)throw new ApiError(503,'TRANSACTION_REQUIRED','Sales correction lost its owned transaction.');return run(database);},database);
  await flushFinanceDeferredEffects(database);const after=await source(database,actor,paymentId);
  if(!after||after.image!==before.image)throw new ApiError(409,'SALES_RECEIPT_SOURCE_CHANGED','The original Sales receipt or invoice allocation changed during correction.');
  const reversed=await database.prepare(`SELECT 1 FROM (${financePaymentReversalSources}) actual WHERE actual.id=? AND actual.company_id=?`).bind(paymentId,actor.companyId).first();
  if(!reversed)throw new ApiError(409,'SALES_RECEIPT_SOURCE_CHANGED','The original Sales correction evidence is incomplete.');
  if(initialAuthority!==await authority(database,actor))throw new ApiError(409,'SALES_POSTING_CONTEXT_CHANGED','The selected Finance authority changed before Sales correction completed.');
  await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:'finance.sales-posting',purpose});return {handled:true as const,result};
 });return resolved.handled?resolved.result:fallback();
}
/** Minimal eligibility for the existing exact Finance payment record. */
export async function salesReceiptReversalPurchase(database:SqlDatabase,actor:WorkContext,paymentId:string){
 const mode=await financePackageEnforcementMode();if(mode==='disabled'||actor.scope.kind!=='holding'||actor.branchId||actor.departmentId)return undefined;
 requirePermission(actor,'finance.read');const current=await requireQueuedCommandAuthority(database,actor,'finance.read');
 const original=await source(database,actor,paymentId);if(!original)return undefined;
 await assertProductionModuleEnabled(database,actor.companyId,'sales');await assertProductionModuleEnabled(database,actor.companyId,'finance');
 const access=(await readPurchasedAccesses(database,mode,actor.companyId,['finance.sales-posting']))['finance.sales-posting'];
 return {capability:'finance.sales-posting' as const,settleExisting:access.settleExisting&&original.status==='settled'&&!samePerson(current,original.created_by)&&hasCorrectionAuthority(current),history:access.history,revision:createHash('sha256').update(JSON.stringify([access.revision,current.iam?.identityVersion,current.iam?.roleRevision])).digest('hex')};
}
function hasCorrectionAuthority(actor:WorkContext){try{requirePermission(actor,'finance.write');requirePermission(actor,'finance.approve');return true;}catch(error){if(error instanceof ApiError&&error.status===403)return false;throw error;}}
