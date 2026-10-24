import 'server-only';
import {createHash,randomUUID} from 'node:crypto';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {requirePermission} from '@erpx/server-identity_access/public/iam-permissions';
import {lockQueuedCommandAuthority,requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {loadIamAuthority} from '@erpx/server-identity_access/public/iam-store';
import {assignmentReadSql} from '@erpx/server-identity_access/public/record-source-authority';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {financePackageEnforcementMode,companyPurchaseReadLocks,readPurchasedAccesses,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure} from '@erpx/server-commercial/public/purchase-command-boundary';
import {packageClockSql,purchasedCapabilityPredicate} from '@erpx/server-commercial/public/purchased-capability-sql';
import {commerceFinanceRecordSources} from '@erpx/server-commerce/public/finance-record-sources';
import {commerceFinanceSalesSources} from '@erpx/server-commerce/public/finance-sales-sources';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import { assertProductionModuleEnabled, scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import {financialConstraintError} from '@erpx/server-workflow/public/command-guards';
import {financeSourceDocumentReadLeases} from '@erpx/server-finance/adapters/document-purchases';
import {financeSalesReceiptOriginalSources} from '@erpx/server-finance/adapters/sales-receipt-sources';
import {financePaymentReversalSources} from '@erpx/server-finance/adapters/supplier-payment-correction-sources';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/adapters/finance-transactions';
import type {SalesPostingSetupProductScope} from '@erpx/server-finance/adapters/sales-posting-setup';

const sales='finance.sales-posting';
async function receiptSource(database:SqlDatabase,actor:WorkContext,id:string,issue:boolean){
 return database.prepare(`SELECT receipt.id,receipt.payment_id,to_jsonb(receipt)::text image,
 EXISTS(SELECT 1 FROM (${financeSalesReceiptOriginalSources}) original JOIN (${commerceFinanceRecordSources.sales.invoice}) owned ON owned.id=original.invoice_id AND owned.company_id=original.company_id
 WHERE original.id=receipt.payment_id AND original.company_id=receipt.company_id AND (original.status='settled' OR EXISTS(SELECT 1 FROM (${financePaymentReversalSources}) reversal WHERE reversal.id=original.id AND reversal.company_id=original.company_id))) sales
 FROM finance_customer_receipts receipt WHERE receipt.${issue?'payment_id':'id'}=? AND receipt.company_id=?`).bind(id,actor.companyId).first<{id:string;payment_id:string;image:string;sales:boolean}>();
}
async function current(database:SqlDatabase,actor:WorkContext,issue:boolean,salesSource:boolean){
 requirePermission(actor,'finance.read');const authority=await requireQueuedCommandAuthority(database,actor,issue?'finance.write':'finance.read');requirePermission(authority,'finance.read');
 await assertProductionModuleEnabled(database,actor.companyId,'finance');if(salesSource)await assertProductionModuleEnabled(database,actor.companyId,'sales');
 return createHash('sha256').update(JSON.stringify([authority.userId,authority.assignmentId,authority.role,authority.authority??null,authority.iam])).digest('hex');
}
async function owned<Result>(actor:WorkContext,id:string,issue:{key:string}|undefined,run:(database:SqlDatabase)=>Promise<Result>){
 requirePermission(actor,'finance.read');
 const owner=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'TRANSACTION_REQUIRED','Customer receipt history requires its original Holding transaction.');
 const mode=await financePackageEnforcementMode();
 try{return await owner.transaction(async database=>{
  await database.batch([...financeSourceDocumentReadLeases(database,actor.companyId,true,'supplier-correction'),database.prepare('LOCK TABLE finance_customer_receipts,finance_periods IN SHARE ROW EXCLUSIVE MODE')]);
  await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(companyPurchaseReadLocks(database,actor.companyId).slice(1));
  // Preserve the original read-only record-scope 404 before checking a stale
  // selected assignment. Values remain private until all final guards pass.
  // These two readers have no audit, storage IO or mutation effects.
  const read=issue?undefined:{result:await run(database)};
  const before=await receiptSource(database,actor,id,!!issue),access=await readPurchasedAccesses(database,mode,actor.companyId,['finance.accounting',sales]);
  const capability=access['finance.accounting'].codes.history==='GRANTED'?'finance.accounting':before?.sales?sales:'finance.accounting';
  const authority=await current(database,actor,!!issue,capability===sales);
  await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability,purpose:'history'});
  const guard=`sales-customer-receipt:${randomUUID()}`,boundary=issue?purchaseCommandBoundary(database,mode,guard,{companyId:actor.companyId,capability,purpose:'history'}):undefined;if(boundary)await database.batch(boundary.before);
  const result=read?read.result:await run(database);await flushFinanceDeferredEffects(database);
  const after=await receiptSource(database,actor,id,!!issue);
  if(before&&(!after||after.image!==before.image||after.sales!==before.sales)||capability===sales&&!after?.sales)throw new ApiError(409,'SALES_RECEIPT_SOURCE_CHANGED','The original immutable customer receipt source changed before completion.');
  if(authority!==await current(database,actor,!!issue,capability===sales))throw new ApiError(409,'SALES_POSTING_CONTEXT_CHANGED','The selected Finance authority changed before customer receipt completion.');
  await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability,purpose:'history'});
  if(boundary){const observed=await database.batch([...boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(guard)]);boundary.observe?.(observed);}return result;
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
export function withSalesCustomerReceiptRead<Result>(actor:WorkContext,id:string,run:(database:SqlDatabase)=>Promise<Result>){return owned(actor,id,undefined,run);}
export function withSalesCustomerReceiptIssue<Result>(actor:WorkContext,paymentId:string,key:string,run:(database:SqlDatabase)=>Promise<Result>){return owned(actor,paymentId,{key},run);}
/** Exact immutable customer-receipt page discovery; no collection or private
 * actor/snapshot is returned to the caller before Work context selection. */
export async function salesCustomerReceiptProductEntry(database:SqlDatabase,scope:SalesPostingSetupProductScope,receiptId:string){
 if(!scope.sessionId||!Number.isSafeInteger(scope.assignmentVersion)||scope.assignmentVersion<1)return null;
 try{await loadIamAuthority(database,scope.companyId,scope.userId,scope.role,scope.sessionId);}catch(error){if(error instanceof ApiError&&(error.status===401||error.status===403))return null;throw error;}
 const access=(await readPurchasedAccesses(database,'enforced',scope.companyId,[sales]))[sales];if(!access.history)return null;
 const row=await database.prepare(`WITH receipt_entry_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms),receipt_scope AS MATERIALIZED(SELECT ?::text company_id,?::text branch_id,?::text department_id)
 SELECT receipt.id FROM finance_customer_receipts receipt CROSS JOIN receipt_scope CROSS JOIN receipt_entry_clock
 JOIN (${financeSalesReceiptOriginalSources}) original ON original.id=receipt.payment_id AND original.company_id=receipt.company_id
 JOIN (${commerceFinanceSalesSources.invoiceScope}) owned ON owned.invoice_id=original.invoice_id AND owned.company_id=original.company_id
 WHERE receipt.id=? AND receipt.company_id=receipt_scope.company_id AND (?::text IS NULL OR owned.branch_id=?) AND (?::text IS NULL OR owned.department_id=?)
 AND EXISTS(SELECT 1 FROM assignments WHERE id=? AND user_id=? AND company_id=receipt_scope.company_id AND role=? AND version=?)
 AND ${assignmentReadSql('receipt_scope','finance.read')}
 AND ${scheduledFeaturePredicate('receipt_scope.company_id','finance')} AND ${scheduledFeaturePredicate('receipt_scope.company_id','sales')}
 AND ${purchasedCapabilityPredicate('receipt_scope.company_id',sales,'history','receipt_entry_clock.now_ms')}
 AND (original.status='settled' OR EXISTS(SELECT 1 FROM (${financePaymentReversalSources}) reversal WHERE reversal.id=original.id AND reversal.company_id=original.company_id))`)
 .bind(scope.companyId,scope.branchId,scope.departmentId,receiptId,scope.branchId,scope.branchId,scope.departmentId,scope.departmentId,scope.assignmentId,scope.userId,scope.role,scope.assignmentVersion,scope.assignmentId,scope.userId,scope.role).first();
 return row?{capability:sales,purchasePurpose:'history' as const}:null;
}
