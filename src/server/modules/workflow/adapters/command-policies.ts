import type {SqlDatabase,SqlStatement} from '../../runtime/public/runtime/types';
import type {CommandActor} from '../domain/commands';
import {invoiceCommandFlow} from '../../commerce/public/invoice-command-policy';
import {expensePaymentCommandFlow} from '../../people_payroll/public/expense-command-policy';
import {prepareFlowCommand} from '../public/flow-routing';
import {createCommandPolicyApplication} from '../application/command-policies';
export function prepareOwnerCommandStatements(db:SqlDatabase,actor:CommandActor,key:string,operation:string,input:unknown,recordId:string,version:number,statements:SqlStatement[]){return createCommandPolicyApplication<SqlStatement>({financeReview:async(...args)=>(await import('../../finance/public/finance-expansion-evidence')).financeExpansionReviewFileSteps(db,...args),invoiceFlow:(...args)=>invoiceCommandFlow(db,...args),expensePaymentFlow:(...args)=>expensePaymentCommandFlow(db,...args),flow:(...args)=>prepareFlowCommand(db,...args)}).prepareOwnerCommandStatements(actor,key,operation,input,recordId,version,statements);}
