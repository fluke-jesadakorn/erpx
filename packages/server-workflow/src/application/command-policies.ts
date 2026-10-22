import type { OwnerCommandFlowTarget } from "@erpx/contracts/command-flow";
import type {CommandActor} from '@erpx/server-workflow/domain/commands';
import {flowCommandTarget} from '@erpx/server-workflow/domain/flow-routing';
import type {FlowKind} from "@erpx/feature-shared/lib/flow";
export interface CommandPolicyPorts<Effect>{
 financeReview(actor:CommandActor,operation:string,recordId:string,version:number,input:unknown,key:string):Promise<Effect[]>;
 invoiceTarget(actor:CommandActor,recordId:string):Promise<OwnerCommandFlowTarget|null>;
 expensePaymentTarget(actor:CommandActor,operation:string,recordId:string):Promise<OwnerCommandFlowTarget|null>;
 flow(actor:CommandActor,key:string,kind:FlowKind,id:string,action:string,input:unknown,version:number):Promise<{before:Effect[];after:Effect[]}>;
}
export function createCommandPolicyApplication<Effect>(ports:CommandPolicyPorts<Effect>){
 async function prepareOwnerCommandStatements(actor:CommandActor,key:string,operation:string,input:unknown,recordId:string,version:number,statements:Effect[]):Promise<Effect[]>{
  if(operation.startsWith('finance.'))statements=[...await ports.financeReview(actor,operation,recordId,version,input,key),...statements];
  const target=operation==='commerce.invoice.create'?await ports.invoiceTarget(actor,recordId):operation.startsWith('expense.payment.')?await ports.expensePaymentTarget(actor,operation,recordId):flowCommandTarget(operation,recordId,input);
  const flow=target?await ports.flow(actor,key,target.kind,target.id,target.action,input,version):null;
  return flow?[...flow.before,...statements,...flow.after]:statements;
 }
 return {prepareOwnerCommandStatements};
}
