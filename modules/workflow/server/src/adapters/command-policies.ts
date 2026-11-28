import type {SqlDatabase,SqlStatement} from '@erpx/platform-runtime-server/public/runtime/types';
import type {CommandActor} from '@erpx/workflow-server/domain/commands';
import {invoiceCommandFlowTarget} from '@erpx/commerce-server/public/invoice-command-policy';
import {expensePaymentCommandFlowTarget} from '@erpx/people-payroll-server/public/expense-command-policy';
import {prepareFlowCommand} from '@erpx/workflow-server/public/flow-command-planning';
import {createCommandPolicyApplication} from '@erpx/workflow-server/application/command-policies';
export function prepareOwnerCommandStatements(db:SqlDatabase,actor:CommandActor,key:string,operation:string,input:unknown,recordId:string,version:number,statements:SqlStatement[]){return createCommandPolicyApplication<SqlStatement>({financeReview:async(...args)=>(await import('@erpx/finance-server/public/finance-expansion-evidence')).financeExpansionReviewFileSteps(db,...args),invoiceTarget:(...args)=>invoiceCommandFlowTarget(db,...args),expensePaymentTarget:(...args)=>expensePaymentCommandFlowTarget(db,...args),flow:(...args)=>prepareFlowCommand(db,...args)}).prepareOwnerCommandStatements(actor,key,operation,input,recordId,version,statements);}
