import type {SqlDatabase,SqlStatement} from '@erpx/server-runtime/public/runtime/types';
import type {CommandActor} from '@erpx/server-workflow/domain/commands';
import {invoiceCommandFlowTarget} from '@erpx/server-commerce/public/invoice-command-policy';
import {expensePaymentCommandFlowTarget} from '@erpx/server-people_payroll/public/expense-command-policy';
import {prepareFlowCommand} from '@erpx/server-workflow/public/flow-routing';
import {createCommandPolicyApplication} from '@erpx/server-workflow/application/command-policies';
export function prepareOwnerCommandStatements(db:SqlDatabase,actor:CommandActor,key:string,operation:string,input:unknown,recordId:string,version:number,statements:SqlStatement[]){return createCommandPolicyApplication<SqlStatement>({financeReview:async(...args)=>(await import('@erpx/server-finance/public/finance-expansion-evidence')).financeExpansionReviewFileSteps(db,...args),invoiceTarget:(...args)=>invoiceCommandFlowTarget(db,...args),expensePaymentTarget:(...args)=>expensePaymentCommandFlowTarget(db,...args),flow:(...args)=>prepareFlowCommand(db,...args)}).prepareOwnerCommandStatements(actor,key,operation,input,recordId,version,statements);}
