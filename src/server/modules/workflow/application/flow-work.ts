import type {WorkContext} from '../../identity_access/public/authorization';import type {FlowKind,FlowView} from "@/lib/shared/flow";import type {WorkItem} from "@/lib/work_projection/work-item";import {BusinessRuleError} from '../../shared';import {flowWorkActionKind,workFlowKind} from '../domain/flow-work';
export interface FlowWorkPorts{prepare(holdingId:string):Promise<void>;resolveFlow(actor:WorkContext,kind:FlowKind,id:string):Promise<FlowView>;requireFlowAccess(actor:WorkContext,kind:FlowKind,id:string):Promise<void>;policyRead<Result>(key:string,read:()=>Promise<Result>):Promise<Result>;workRows(actor:WorkContext,kind:'order'|'invoice'|'journal'|'payroll',reference:boolean):Promise<Record<string,string|number|null>[]>;}
export function createFlowWorkApplication(ports:FlowWorkPorts){
async function enrichFlowWork(actor:WorkContext,items:WorkItem[]) {
  await ports.prepare(actor.holdingId);
  return Promise.all(items.map(async item => {
    // These domains own their review and handoff states.
    if (["finance_review","customer_identity","customer_duplicate","customer_access","support_incident","support_policy","support_sla_alert", "support_ticket", "support_session", "maintenance_work", "maintenance_plan", "project_handoff", "field_service", "collaboration_action", "operational_exception", "learning_assignment", "benefit_enrollment", "talent_application", "marketing_candidate", "manufacturing_order", "crm_lead", "crm_activity", "customer_followup", "legal", "people_lifecycle", "organization_change", "company_business_acceptance", "pos", "pms", "overtime", "refund", "supplier_batch", "quality_hold", "quality_alert", "inventory_transit", "supplier_promise", "procurement_rfq", "supplier_bill"].includes(item.type)) return item;
    const flow = item.flow ?? await ports.resolveFlow(actor,workFlowKind(item.type),item.id);
    const primary = flow.current?.primary;
    return {...item,flow, canAct: flow.current ? flow.canAct : item.canAct ?? item.queue === "todo", category: flow.blocker && !flow.current?.primary ? "setup" as const : "operational" as const, setupReason: flow.blocker, nextAction:flow.current?.action ?? item.nextAction, responsibleQueue:primary ? `${flow.current!.teamName} / ${primary.name}` : flow.current?.teamName ?? item.responsibleQueue,blocker:flow.blocker ?? item.blocker,
      queue: flow.current ? flow.canAct ? "todo" as const : "waiting" as const : item.queue,
      relationships:[...new Set([...(item.relationships ?? []).filter(r => !["assigned","approval"].includes(r) || flow.canAct),...(primary?.userId===actor.userId ? ["assigned" as const] : [])])]};
  }));
}

/** Commercial and financial owner documents that belong in My Work. */
async function otherFlowWork(actor:WorkContext, reference = false):Promise<WorkItem[]> {
  if (actor.beta) return [];
  await ports.prepare(actor.holdingId);const items:WorkItem[]=[];
  const sources = [
    ["order","number","updated_at"],
    ["invoice","invoice_number","updated_at"],
    ["journal","journal_number","created_at"],
    ["payroll","pay_date","updated_at"],
  ] as const;
  for (const [kind,title,updated] of sources) {
    let rows:Record<string,string|number|null>[];
    try{rows=await ports.workRows(actor,kind,reference);}catch(error){if(error instanceof BusinessRuleError&&[403,404].includes(error.status))continue;throw error;}
    await Promise.all(rows.map(row => ports.policyRead( JSON.stringify(["flow-record",actor.companyId,kind,String(row.id)]), async () => row)));
    const readRow = async (row: Record<string,string|number|null>): Promise<WorkItem | null> => {
      if (row.archived_at || kind === "invoice" && row.source_type === "expense_claim") return null;
      try { if (reference) await ports.requireFlowAccess(actor,kind,String(row.id)); } catch (error) { if (error instanceof BusinessRuleError && [403,404].includes(error.status)) return null; throw error; }
      const flow = await ports.resolveFlow(actor,kind,String(row.id));
      const own = row.created_by === actor.userId;
      const participant = flow.steps.some(s => s.primary?.userId === actor.userId) || flow.history.some(e => e.actorName === actor.userId);
      if (!own && !participant && !(flow.canManage && flow.blocker)) return null;
      const href = kind === "order" ? `/commerce/orders/${row.id}` : kind === "invoice" ? `/finance/invoices/${row.id}` : kind === "journal" ? `/finance/journals/${row.id}` : `/people/payroll/runs/${encodeURIComponent(String(row.id))}`;
      return {id:String(row.id),type:kind,actionKind:flowWorkActionKind(flow),title:String(row[title]),detail:flow.current?.action ?? "No pending action",href,status:flow.state,updatedAt:Number(row[updated]),queue:flow.canAct ? "todo" : flow.current ? "waiting" : "completed",flow,canAct:flow.canAct,category:flow.blocker && !flow.current?.primary ? "setup" : "operational",setupReason:flow.blocker,relationships:[...(own ? ["requested" as const] : []),...(flow.canAct ? ["assigned" as const] : [])]};
    };
    // Bound pooled read concurrency while retaining source order and every row.
    // The authorized reference reader stays sequential for parity measurements.
    const width = reference ? 1 : 8;
    for (let offset=0;offset<rows.length;offset+=width) {
      const batch = await Promise.all(rows.slice(offset,offset+width).map(readRow));
      items.push(...batch.filter((item): item is WorkItem => item !== null));
    }
  }
  return items;
}

return {enrichFlowWork,otherFlowWork};}
