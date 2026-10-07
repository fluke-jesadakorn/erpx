import type {CommandActor} from '../domain/commands';
import {flowCommandTarget} from '../domain/flow-routing';
import type {FlowKind} from '@/lib/flow';
export interface CommandPolicyPorts<Effect>{
 financeReview(actor:CommandActor,operation:string,recordId:string,version:number,input:unknown,key:string):Promise<Effect[]>;
 invoiceFlow(actor:CommandActor,key:string,recordId:string,input:unknown,version:number):Promise<{before:Effect[];after:Effect[]}|null>;
 expensePaymentFlow(actor:CommandActor,key:string,operation:string,recordId:string,input:unknown,version:number):Promise<{before:Effect[];after:Effect[]}|null>;
 flow(actor:CommandActor,key:string,kind:FlowKind,id:string,action:string,input:unknown,version:number):Promise<{before:Effect[];after:Effect[]}>;
}
export function createCommandPolicyApplication<Effect>(ports:CommandPolicyPorts<Effect>){
 async function prepareOwnerCommandStatements(actor:CommandActor,key:string,operation:string,input:unknown,recordId:string,version:number,statements:Effect[]):Promise<Effect[]>{
  if(operation.startsWith('finance.'))statements=[...await ports.financeReview(actor,operation,recordId,version,input,key),...statements];
  const target=flowCommandTarget(operation,recordId,input);
  const flow=operation==='commerce.invoice.create'?await ports.invoiceFlow(actor,key,recordId,input,version):operation.startsWith('expense.payment.')?await ports.expensePaymentFlow(actor,key,operation,recordId,input,version):target?await ports.flow(actor,key,target.kind,target.id,target.action,input,version):null;
  return flow?[...flow.before,...statements,...flow.after]:statements;
 }
 return {prepareOwnerCommandStatements};
}
