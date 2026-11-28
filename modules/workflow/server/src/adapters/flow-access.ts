import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import type {FlowKind} from "@erpx/platform-shared-ui/lib/flow";
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {holdingResources} from '../../../../../platform/runtime/server/src/public/runtime';
import {createFlowAccessApplication} from '@erpx/workflow-server/application/flow-access';
import {flowRecord} from '@erpx/workflow-server/public/flow-record';
export async function requireFlowAccess(actor:WorkContext,kind:FlowKind,id:string,provided?:SqlDatabase){
 let write=provided!;
 await createFlowAccessApplication({prepare:async holding=>{if(!provided)write=(await holdingResources(holding)).write;},ownerRead:async(actor,kind,id)=>{
  switch(kind){
   case 'expense':await (await import("@erpx/people-payroll-server/public/expense-read")).getExpense(actor,id,write);break;
   case 'leave':await (await import("@erpx/people-payroll-server/public/leave-read")).getLeave(actor.holdingId,id,actor,write);break;
   case 'staff_task':await (await import('@erpx/operations-server/public/staff-tasks')).taskDetail(write,actor,id);break;
   case 'request':await (await import("@erpx/records-server/public/employee-requests-read")).getEmployeeRequest(write,actor,id);break;
   case 'stock_count':await (await import('@erpx/inventory-server/public/inventory-counts')).getCount(actor,id,write);break;
   case 'transfer':await (await import('@erpx/inventory-server/public/inventory-transfer-read')).getTransfer(actor,id);break;
   case 'order':await (await import('@erpx/commerce-server/public/commerce')).getOrder(actor,id,write);break;
   case 'invoice':await (await import('@erpx/finance-server/public/finance-invoice-read')).getInvoice(actor,id,write);break;
   case 'journal':await (await import('@erpx/finance-server/public/ledger')).journalDetail(actor,id,write);break;
   case 'payroll':{const run=await (await import('@erpx/people-payroll-ui/public/native-payroll-summary-owned')).ownedPeoplePayrollRun(actor,actor.holdingSlug,id);if(run===undefined)throw new ApiError(503,'NATIVE_PEOPLE_PAYROLL_UNAVAILABLE','The native People Payroll reader is unavailable.');break;}
   case 'record':{const row=await flowRecord(write,actor.companyId,kind,id);await (await import('@erpx/records-server/public/records')).getRecord(actor,String(row.module_id),id,write);break;}
  }
 }}).requireFlowAccess(actor,kind,id);
 return write;
}
