import type {WorkContext} from '../../identity_access/public/authorization';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {FlowKind} from '@/lib/flow';
import {holdingResources} from '../../runtime/public/runtime';
import {createFlowAccessApplication} from '../application/flow-access';
import {flowRecord} from '../public/flow-routing';
export async function requireFlowAccess(actor:WorkContext,kind:FlowKind,id:string,provided?:SqlDatabase){
 let write=provided!;
 await createFlowAccessApplication({prepare:async holding=>{if(!provided)write=(await holdingResources(holding)).write;},ownerRead:async(actor,kind,id)=>{
  switch(kind){
   case 'expense':await (await import('../../people_payroll/public/expenses')).getExpense(actor,id,write);break;
   case 'leave':await (await import('../../people_payroll/public/leave')).getLeave(actor.holdingId,id,actor,write);break;
   case 'staff_task':await (await import('../../operations/public/staff-tasks')).taskDetail(write,actor,id);break;
   case 'request':await (await import('../../records/public/employee-requests')).getEmployeeRequest(write,actor,id);break;
   case 'stock_count':await (await import('../../inventory/public/inventory-counts')).getCount(actor,id,write);break;
   case 'transfer':await (await import('../../inventory/public/inventory-transfers')).getTransfer(actor,id,write);break;
   case 'order':await (await import('../../commerce/public/commerce')).getOrder(actor,id,write);break;
   case 'invoice':await (await import('../../finance/public/finance')).getInvoice(actor,id,write);break;
   case 'journal':await (await import('../../finance/public/ledger')).journalDetail(actor,id,write);break;
   case 'payroll':await (await import('../../people_payroll/public/people-payroll')).getPayrollRun(actor,id,write);break;
   case 'record':{const row=await flowRecord(write,actor.companyId,kind,id);await (await import('../../records/public/records')).getRecord(actor,String(row.module_id),id,write);break;}
  }
 }}).requireFlowAccess(actor,kind,id);
 return write;
}
