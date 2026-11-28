import 'server-only';
import { ppRd29FinanceExpenseTreasurySql } from './pp-read-tail-r29-sources';
import { nativePeopleBatchA } from '@erpx/collaboration-server/adapters/native-people-batch-a';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {holdingResources} from '../../../../../platform/runtime/server/src/public/runtime';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {requirePermission} from '@erpx/identity-access-server/public/iam-permissions';
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {staffGuard} from '@erpx/operations-server/public/staff-policy';
import {createCancelExpensePayment} from '@erpx/people-payroll-server/application/expense-payment-cancellation';
import {expensePaymentCancellationSchema,type ExpensePaymentCancellationInput} from '@erpx/people-payroll-server/domain/expense-payment-cancellation';
import {expenseMutation,expenseCommandPort} from '@erpx/people-payroll-server/adapters/expense-command-port';
import {paymentFromDatabase} from '@erpx/people-payroll-server/public/expense-payment-read';
import {expensePaymentTreasurySql} from '@erpx/people-payroll-server/adapters/expense-payment-cancellation-sources';
import {ppRd52PaymentImage,ppRd52PaymentValidity} from './pp-read-tail-r52-sources';
import {expensePayers} from '@erpx/people-payroll-server/adapters/expense-workflow-read';
import {expenseAuthoritySnapshot} from '@erpx/people-payroll-server/adapters/expense-routing';
import {withExpensePurchasedCommand,expenseFinalRead} from '@erpx/people-payroll-server/adapters/expense-purchases';
import {expensePaymentDraftExpenseId} from '@erpx/people-payroll-server/adapters/document-page-payment-source';
export {expensePaymentCancellationSchema} from '@erpx/people-payroll-server/domain/expense-payment-cancellation';
export type {ExpensePaymentCancellationInput,ExpensePaymentCancellationResult} from '@erpx/people-payroll-server/domain/expense-payment-cancellation';

function nativeCancellation(database:SqlDatabase){
 const payment=async(actor:WorkContext,id:string)=>{
   const draft=await expensePaymentDraftExpenseId(database,id,actor.companyId);
   if(!draft)throw new ApiError(404,'PAYMENT_DRAFT_NOT_FOUND','Payment preparation not found in this Company.');
   // Canonical parent -> child row order, before the native command receipt.
   await database.prepare('SELECT id FROM finance_expenses WHERE id=? AND company_id=? FOR UPDATE').bind(draft.expense_id,actor.companyId).first();
   await database.prepare('SELECT id FROM expense_payment_drafts WHERE id=? AND company_id=? FOR UPDATE').bind(id,actor.companyId).first();
   return paymentFromDatabase(database,actor,id);
  }
 return createCancelExpensePayment({
  ...expenseCommandPort(database),authorize:actor=>{requirePermission(actor,'expense.pay');},
  payment,
  history:async(actor,expenseId,id,input)=>{
   const read=await payment(actor,id);
   if(read.expense.id!==expenseId||read.draft.expense_id!==expenseId||read.draft.status!=='cancelled'||Number(read.draft.version)<input.expectedVersion+1||!(await expensePayers(database,read.expense)).includes(actor.userId))throw new ApiError(403,'EXPENSE_PAYMENT_HISTORY_UNAVAILABLE','The cancelled payment preparation is not available to the current independent Treasury role.');
   const current=await database.prepare(ppRd52PaymentImage(true)).bind(id,expenseId,actor.companyId,actor.assignmentId,actor.userId).first<{draft:string;expense:string}>();
   if(!current)throw new ApiError(409,'EXPENSE_PAYMENT_HISTORY_UNAVAILABLE','The exact native payment cancellation history is unavailable.');
   const authority=await expenseAuthoritySnapshot(database,actor.companyId);
   expenseFinalRead(database,actor,async()=>{const valid=await database.prepare(ppRd52PaymentValidity(false,'','',authority.query)).bind(id,actor.companyId,current.draft,current.expense,actor.assignmentId,actor.userId,...authority.args).first<boolean>('ok');if(valid!==true)throw new ApiError(409,'EXPENSE_PAYMENT_CHANGED','The original cancellation history or Treasury authority changed before recovery completed.');});
  },payers:row=>expensePayers(database,row),
  prepare:async(actor,id,expenseId,input,key)=>{
   const now=Date.now(),source=await database.prepare(ppRd52PaymentImage(false)).bind(id,expenseId,actor.companyId).first<{draft:string;expense:string}>();
   if(!source)throw new ApiError(409,'EXPENSE_PAYMENT_CHANGED','The original payment preparation changed.');
   const authority=await expenseAuthoritySnapshot(database,actor.companyId),expectedDraft={...JSON.parse(source.draft),status:'cancelled',version:input.expectedVersion+1,updated_at:now};
   const independent=ppRd29FinanceExpenseTreasurySql(expensePaymentTreasurySql('e'));
   const independentArgs=[expenseId,actor.companyId,actor.assignmentId,actor.userId];
   const nativeSource=`EXISTS(SELECT 1 FROM expense_payment_drafts d JOIN command_receipts prepared ON prepared.operation='expense.payment.prepare' AND prepared.record_id=d.expense_id
    AND prepared.actor_id=d.created_by AND prepared.authority_json::jsonb->>'companyId'=d.company_id AND d.id='expense-payment-'||prepared.command_id
    AND prepared.result_json::jsonb->>'id'=d.expense_id AND prepared.result_json::jsonb->>'draftId'=d.id
    WHERE d.id=? AND d.company_id=? AND d.payment_id IS NULL
     AND NOT EXISTS(SELECT 1 FROM command_receipts confirmed WHERE confirmed.operation='expense.payment.confirm' AND confirmed.record_id=d.id))`;
   expenseFinalRead(database,actor,async()=>{
    const final=await database.prepare(ppRd52PaymentValidity(true,nativeSource,independent,authority.query)).bind(id,actor.companyId,JSON.stringify(expectedDraft),source.expense,id,actor.companyId,...independentArgs,...authority.args).first<boolean>('ok');
    if(final!==true)throw new ApiError(409,'EXPENSE_PAYMENT_CHANGED','The payment source or independent Treasury authority changed before cancellation completed.');
   });
   return expenseMutation({id,expenseId,status:'cancelled' as const,version:input.expectedVersion+1,expenseVersion:input.expenseVersion,reason:input.reason,paymentNotExecuted:true as const},[
    staffGuard(database,key,actor.companyId,actor.userId),authority.guard(key),
    assertion(database,key,'payment-cancel-parent','EXISTS(SELECT 1 FROM finance_expenses e WHERE id=? AND company_id=? AND version=? AND to_jsonb(e)=?::jsonb)',[expenseId,actor.companyId,input.expenseVersion,source.expense]),
    assertion(database,key,'payment-cancel-draft',"EXISTS(SELECT 1 FROM expense_payment_drafts d WHERE id=? AND company_id=? AND expense_id=? AND version=? AND status='draft' AND payment_id IS NULL AND to_jsonb(d)=?::jsonb)",[id,actor.companyId,expenseId,input.expectedVersion,source.draft]),
    assertion(database,key,'payment-cancel-native',nativeSource,[id,actor.companyId]),assertion(database,key,'payment-cancel-treasury',independent,independentArgs),
    await nativePeopleBatchA(database).cancelPayment({companyId:actor.companyId,now,id,expectedVersion:input.expectedVersion}),
   ]);
  },
 });
}
export function nativeExpensePaymentCancellation(database:SqlDatabase){
 return (actor:WorkContext,expenseId:string,id:string,input:ExpensePaymentCancellationInput,key:string)=>withExpensePurchasedCommand(database,actor,{key,purpose:'settle-existing',requireUnit:true},tx=>nativeCancellation(tx)(actor,expenseId,id,input,key));
}
export async function cancelExpensePayment(actor:WorkContext,expenseId:string,id:string,raw:ExpensePaymentCancellationInput,key:string){
 const input=expensePaymentCancellationSchema.parse(raw);requirePermission(actor,'expense.pay');
 return nativeExpensePaymentCancellation((await holdingResources(actor.holdingId)).write)(actor,expenseId,id,input,key);
}
