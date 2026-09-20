import 'server-only';
import { z } from 'zod';
import { flowKinds, type FlowKind } from '@/lib/flow';
import type { WorkContext } from './authorization';
import { holdingResources } from './runtime';
import { companyAuthority } from './staff-policy';
import { requireFlowAccess } from './flow-access';
import { resolveFlow } from './flow-routing';
import { ApiError } from './http';

const sources:Record<FlowKind,string>={expense:'finance_expenses',leave:'leave_requests',staff_task:'staff_tasks',request:'employee_requests',stock_count:'inventory_counts',transfer:'inventory_transfers',order:'commerce_orders',invoice:'finance_invoices',journal:'finance_journals',payroll:'hr_payroll_runs',record:'workspace_records'};
export const routingQuerySchema=z.object({kind:z.enum(flowKinds).default('expense'),after:z.string().max(180).default(''),limit:z.coerce.number().int().min(1).max(50).default(25)});
export async function requireRoutingManager(actor:WorkContext) {
 const {write}=await holdingResources(actor.holdingId);
 if(actor.branchId||actor.departmentId||!await companyAuthority(write,actor))throw new ApiError(403,'ROUTING_MANAGER_REQUIRED','Use an active Company organization-management assignment to inspect routing.');
 return write;
}
/** Keyset scan is separate from My Work limits. Only authorized blocked records count toward a page. */
export async function routingAttention(actor:WorkContext,input:unknown) {
 const query=routingQuerySchema.parse(input),write=await requireRoutingManager(actor);
 const items:Array<{id:string;kind:FlowKind;title:string;state:string;blocker:string;action:string|null;team:string|null;recordVersion:number;routeVersion:number}>=[];
 let after=query.after,exhausted=false;
 while(!exhausted&&items.length<=query.limit) {
  const rows=(await write.prepare(`SELECT * FROM ${sources[query.kind]} WHERE company_id=? AND id>? ORDER BY id LIMIT 100`).bind(actor.companyId,after).all<Record<string,string|number|null>>()).results;
  if(!rows.length)break;
  for(const row of rows) {
   after=String(row.id);
   if(row.archived_at)continue;
   try {await requireFlowAccess(actor,query.kind,after);}catch(error){if(error instanceof ApiError&&[403,404].includes(error.status))continue;throw error;}
   // Expense payables are routed by the expense owner, not the generic invoice workflow.
   if(query.kind==='invoice'&&row.source_type==='expense_claim')continue;
   const flow=await resolveFlow(write,actor,query.kind,after);
   if(!flow.canManage||!flow.blocker)continue;
   items.push({id:after,kind:query.kind,title:String(row.title||row.number||row.invoice_number||row.journal_number||row.merchant||row.pay_date||after),state:flow.state,blocker:flow.blocker,action:flow.current?.action??null,team:flow.current?.teamName??null,recordVersion:flow.recordVersion,routeVersion:flow.version});
   if(items.length>query.limit)break;
  }
  exhausted=rows.length<100;
 }
 const more=items.length>query.limit,visible=items.slice(0,query.limit);
 return {kind:query.kind,items:visible,nextAfter:more?visible.at(-1)!.id:null,checkedAt:Date.now(),coverage:'named_flow' as const};
}
