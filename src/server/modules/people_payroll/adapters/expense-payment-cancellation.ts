import 'server-only';
import type {WorkContext} from '../../identity_access/public/authorization';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {holdingResources} from '../../runtime/public/runtime';
import {ApiError} from '../../runtime/public/http';
import {requirePermission} from '../../identity_access/public/iam-permissions';
import {assertion} from '../../workflow/public/commands';
import {staffGuard} from '../../operations/public/staff-policy';
import {createCancelExpensePayment} from '../application/expense-payment-cancellation';
import {expensePaymentCancellationSchema,type ExpensePaymentCancellationInput} from '../domain/expense-payment-cancellation';
import {expenseMutation,expenseCommandPort} from './expense-command-port';
import {paymentFromDatabase} from './expense-payments';
import {expensePaymentCancellations,expensePaymentTreasurySql} from './expense-payment-cancellation-sources';
import {expensePayers} from './expense-workflow';
import {expenseAuthoritySnapshot} from './expense-routing';
import {withExpensePurchasedCommand,expenseFinalRead} from './expense-purchases';
export {expensePaymentCancellationSchema} from '../domain/expense-payment-cancellation';
export type {ExpensePaymentCancellationInput,ExpensePaymentCancellationResult} from '../domain/expense-payment-cancellation';

function nativeCancellation(database:SqlDatabase){
 const payment=async(actor:WorkContext,id:string)=>{
   const draft=await database.prepare('SELECT expense_id FROM expense_payment_drafts WHERE id=? AND company_id=?').bind(id,actor.companyId).first<{expense_id:string}>();
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
   const current=await database.prepare(`SELECT to_jsonb(d)::text draft,to_jsonb(e)::text expense FROM expense_payment_drafts d JOIN finance_expenses e ON e.id=d.expense_id AND e.company_id=d.company_id
    WHERE d.id=? AND d.expense_id=? AND d.company_id=? AND EXISTS(SELECT 1 FROM (${expensePaymentCancellations}) terminal WHERE terminal.id=d.id AND terminal.company_id=d.company_id)
    AND (${expensePaymentTreasurySql('e')})`).bind(id,expenseId,actor.companyId,actor.assignmentId,actor.userId).first<{draft:string;expense:string}>();
   if(!current)throw new ApiError(409,'EXPENSE_PAYMENT_HISTORY_UNAVAILABLE','The exact native payment cancellation history is unavailable.');
   const authority=await expenseAuthoritySnapshot(database,actor.companyId);
   expenseFinalRead(database,actor,async()=>{const valid=await database.prepare(`SELECT (EXISTS(SELECT 1 FROM expense_payment_drafts d JOIN finance_expenses e ON e.id=d.expense_id AND e.company_id=d.company_id WHERE d.id=? AND d.company_id=? AND to_jsonb(d)=?::jsonb AND to_jsonb(e)=?::jsonb AND (${expensePaymentTreasurySql('e')})) AND (${authority.query})) ok`).bind(id,actor.companyId,current.draft,current.expense,actor.assignmentId,actor.userId,...authority.args).first<boolean>('ok');if(valid!==true)throw new ApiError(409,'EXPENSE_PAYMENT_CHANGED','The original cancellation history or Treasury authority changed before recovery completed.');});
  },payers:row=>expensePayers(database,row),
  prepare:async(actor,id,expenseId,input,key)=>{
   const now=Date.now(),source=await database.prepare(`SELECT to_jsonb(d)::text draft,to_jsonb(e)::text expense FROM expense_payment_drafts d JOIN finance_expenses e ON e.id=d.expense_id AND e.company_id=d.company_id WHERE d.id=? AND d.expense_id=? AND d.company_id=?`).bind(id,expenseId,actor.companyId).first<{draft:string;expense:string}>();
   if(!source)throw new ApiError(409,'EXPENSE_PAYMENT_CHANGED','The original payment preparation changed.');
   const authority=await expenseAuthoritySnapshot(database,actor.companyId),expectedDraft={...JSON.parse(source.draft),status:'cancelled',version:input.expectedVersion+1,updated_at:now};
   const independent=`EXISTS(SELECT 1 FROM finance_expenses e WHERE e.id=? AND e.company_id=? AND (${expensePaymentTreasurySql('e')}))`;
   const independentArgs=[expenseId,actor.companyId,actor.assignmentId,actor.userId];
   const nativeSource=`EXISTS(SELECT 1 FROM expense_payment_drafts d JOIN command_receipts prepared ON prepared.operation='expense.payment.prepare' AND prepared.record_id=d.expense_id
    AND prepared.actor_id=d.created_by AND prepared.authority_json::jsonb->>'companyId'=d.company_id AND d.id='expense-payment-'||prepared.command_id
    AND prepared.result_json::jsonb->>'id'=d.expense_id AND prepared.result_json::jsonb->>'draftId'=d.id
    WHERE d.id=? AND d.company_id=? AND d.payment_id IS NULL
     AND NOT EXISTS(SELECT 1 FROM command_receipts confirmed WHERE confirmed.operation='expense.payment.confirm' AND confirmed.record_id=d.id))`;
   expenseFinalRead(database,actor,async()=>{
    const final=await database.prepare(`SELECT (EXISTS(SELECT 1 FROM expense_payment_drafts d JOIN finance_expenses e ON e.id=d.expense_id AND e.company_id=d.company_id WHERE d.id=? AND d.company_id=? AND to_jsonb(d)=?::jsonb AND to_jsonb(e)=?::jsonb)
     AND (${nativeSource}) AND (${independent}) AND (${authority.query})) ok`).bind(id,actor.companyId,JSON.stringify(expectedDraft),source.expense,id,actor.companyId,...independentArgs,...authority.args).first<boolean>('ok');
    if(final!==true)throw new ApiError(409,'EXPENSE_PAYMENT_CHANGED','The payment source or independent Treasury authority changed before cancellation completed.');
   });
   return expenseMutation({id,expenseId,status:'cancelled' as const,version:input.expectedVersion+1,expenseVersion:input.expenseVersion,reason:input.reason,paymentNotExecuted:true as const},[
    staffGuard(database,key,actor.companyId,actor.userId),authority.guard(key),
    assertion(database,key,'payment-cancel-parent','EXISTS(SELECT 1 FROM finance_expenses e WHERE id=? AND company_id=? AND version=? AND to_jsonb(e)=?::jsonb)',[expenseId,actor.companyId,input.expenseVersion,source.expense]),
    assertion(database,key,'payment-cancel-draft',"EXISTS(SELECT 1 FROM expense_payment_drafts d WHERE id=? AND company_id=? AND expense_id=? AND version=? AND status='draft' AND payment_id IS NULL AND to_jsonb(d)=?::jsonb)",[id,actor.companyId,expenseId,input.expectedVersion,source.draft]),
    assertion(database,key,'payment-cancel-native',nativeSource,[id,actor.companyId]),assertion(database,key,'payment-cancel-treasury',independent,independentArgs),
    database.prepare("UPDATE expense_payment_drafts SET status='cancelled',version=version+1,updated_at=? WHERE id=? AND company_id=? AND version=? AND status='draft' AND payment_id IS NULL").bind(now,id,actor.companyId,input.expectedVersion),
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
