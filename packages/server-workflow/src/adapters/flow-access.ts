import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {FlowKind} from "@erpx/feature-shared/lib/flow";
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {createFlowAccessApplication} from '@erpx/server-workflow/application/flow-access';
import {flowRecord} from '@erpx/server-workflow/public/flow-record';
export async function requireFlowAccess(actor:WorkContext,kind:FlowKind,id:string,provided?:SqlDatabase){
 let write=provided!;
 await createFlowAccessApplication({prepare:async holding=>{if(!provided)write=(await holdingResources(holding)).write;},ownerRead:async(actor,kind,id)=>{
  switch(kind){
   case 'expense':await (await import("@erpx/server-people_payroll/public/expense-read")).getExpense(actor,id,write);break;
   case 'leave':await (await import("@erpx/server-people_payroll/public/leave-read")).getLeave(actor.holdingId,id,actor,write);break;
   case 'staff_task':await (await import('@erpx/server-operations/public/staff-tasks')).taskDetail(write,actor,id);break;
   case 'request':await (await import("@erpx/server-records/public/employee-requests-read")).getEmployeeRequest(write,actor,id);break;
   case 'stock_count':await (await import('@erpx/server-inventory/public/inventory-counts')).getCount(actor,id,write);break;
   case 'transfer':await (await import('@erpx/server-inventory/public/inventory-transfers')).getTransfer(actor,id,write);break;
   case 'order':await (await import('@erpx/server-commerce/public/commerce')).getOrder(actor,id,write);break;
   case 'invoice':await (await import('@erpx/server-finance/public/finance-invoice-read')).getInvoice(actor,id,write);break;
   case 'journal':await (await import('@erpx/server-finance/public/ledger')).journalDetail(actor,id,write);break;
   case 'payroll':await (await import('@erpx/server-people_payroll/public/people-payroll')).getPayrollRun(actor,id,write);break;
   case 'record':{const row=await flowRecord(write,actor.companyId,kind,id);await (await import('@erpx/server-records/public/records')).getRecord(actor,String(row.module_id),id,write);break;}
  }
 }}).requireFlowAccess(actor,kind,id);
 return write;
}
