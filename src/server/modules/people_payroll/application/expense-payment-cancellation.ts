import type {WorkContext} from '../../identity_access/public/authorization';
import {samePerson} from '@/lib/identity_access/iam';
import type {ExpenseCommandPort,ExpenseMutation,ExpenseRecord} from './expense-commands';
import {expensePaymentCancellationSchema,type ExpensePaymentCancellationInput,type ExpensePaymentCancellationResult} from '../domain/expense-payment-cancellation';
export interface ExpensePaymentCancellationPorts extends ExpenseCommandPort{
 authorize(actor:WorkContext):void;
 history(actor:WorkContext,expenseId:string,id:string,input:ExpensePaymentCancellationInput):Promise<void>;
 payment(actor:WorkContext,id:string):Promise<{draft:ExpenseRecord;expense:ExpenseRecord}>;
 payers(expense:ExpenseRecord):Promise<string[]>;
 prepare(actor:WorkContext,id:string,expenseId:string,input:ExpensePaymentCancellationInput,key:string,draft:ExpenseRecord,expense:ExpenseRecord):Promise<ExpenseMutation<ExpensePaymentCancellationResult>>;
}
export function createCancelExpensePayment(ports:ExpensePaymentCancellationPorts){
 return async(actor:WorkContext,expenseId:string,id:string,raw:ExpensePaymentCancellationInput,key:string)=>{
  const input=expensePaymentCancellationSchema.parse(raw);ports.authorize(actor);
  const previous=await ports.replay(actor,key,'expense.payment.cancel',id,input);if(previous){await ports.history(actor,expenseId,id,input);return previous;}
  const {draft,expense}=await ports.payment(actor,id);
  if(draft.company_id!==actor.companyId||expense.company_id!==actor.companyId||draft.expense_id!==expenseId||expense.id!==expenseId)throw ports.error({status:404,code:'PAYMENT_DRAFT_NOT_FOUND',message:'Payment preparation not found in this claim.'});
  if(Number(draft.version)!==input.expectedVersion||Number(expense.version)!==input.expenseVersion)throw ports.error({status:409,code:'EXPENSE_PAYMENT_CHANGED',message:'The claim or payment preparation changed. Review its current status.'});
  if(draft.status!=='draft'||draft.payment_id!==null||expense.workflow_version!==2||!expense.invoice_id||!expense.journal_id||!['posted','reimbursed','cancelled'].includes(String(expense.status)))throw ports.error({status:409,code:'EXPENSE_PAYMENT_NOT_CANCELLABLE',message:'Only an unconfirmed native payment preparation can be cancelled. Recorded bank payments must retain their settlement history.'});
  const accounting=JSON.parse(String(expense.accounting_json??'{}')) as {preparedBy?:string};
  if([expense.claimant_user_id,expense.created_by,expense.prepared_by,accounting.preparedBy].some(value=>typeof value==='string'&&samePerson(actor,value))||!(await ports.payers(expense)).includes(actor.userId))throw ports.error({status:403,code:'TREASURY_REQUIRED',message:'An independent current Treasury operator must cancel this payment preparation.'});
  const mutation=await ports.prepare(actor,id,expenseId,input,key,draft,expense);
  return ports.commit(actor,key,'expense.payment.cancel',input,id,input.expectedVersion+1,mutation,{companyId:actor.companyId,branchId:expense.branch_id?String(expense.branch_id):null,departmentId:expense.department_id?String(expense.department_id):null});
 };
}
