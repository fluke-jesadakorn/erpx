import type { WorkContext } from "./authorization";
import type { FlowKind } from "@/lib/flow";
import type { WorkItem } from "@/lib/work-item";
import { holdingResources } from "./runtime";
import { requireFlowAccess } from "./flow-access";
import { policyRead } from "./iam-read-snapshot";
import { resolveFlow } from "./flow-routing";
import { ApiError } from "./http";

export function workFlowKind(type:WorkItem["type"]):FlowKind {
  return ["service_request","equipment_request","purchase_request"].includes(type) ? "request" : type as FlowKind;
}

export async function enrichFlowWork(actor:WorkContext,items:WorkItem[]) {
  const {write} = await holdingResources(actor.holdingId);
  return Promise.all(items.map(async item => {
    // These domains own their review and handoff states.
    if (["customer_identity","customer_duplicate","customer_access","support_incident","support_policy","support_sla_alert", "support_ticket", "support_session", "maintenance_work", "field_service", "collaboration_action", "learning_assignment", "benefit_enrollment", "talent_application", "marketing_candidate", "manufacturing_order", "crm_lead", "customer_followup", "legal", "people_lifecycle", "organization_change", "pos", "pms", "overtime", "refund", "supplier_batch", "quality_hold", "quality_alert", "inventory_transit", "supplier_promise", "procurement_rfq", "supplier_bill"].includes(item.type)) return item;
    const flow = item.flow ?? await resolveFlow(write,actor,workFlowKind(item.type),item.id);
    const primary = flow.current?.primary;
    return {...item,flow, canAct: flow.current ? flow.canAct : item.canAct ?? item.queue === "todo", category: flow.blocker && !flow.current?.primary ? "setup" as const : "operational" as const, setupReason: flow.blocker, nextAction:flow.current?.action ?? item.nextAction, responsibleQueue:primary ? `${flow.current!.teamName} / ${primary.name}` : flow.current?.teamName ?? item.responsibleQueue,blocker:flow.blocker ?? item.blocker,
      queue: flow.current ? flow.canAct ? "todo" as const : "waiting" as const : item.queue,
      relationships:[...new Set([...(item.relationships ?? []).filter(r => !["assigned","approval"].includes(r) || flow.canAct),...(primary?.userId===actor.userId ? ["assigned" as const] : [])])]};
  }));
}

/** Commercial and financial owner documents that belong in My Work. */
export async function otherFlowWork(actor:WorkContext, reference = false):Promise<WorkItem[]> {
  if (actor.beta) return [];
  const {write} = await holdingResources(actor.holdingId), items:WorkItem[]=[];
  const sources = [
    ["order","commerce_orders","number","updated_at"],
    ["invoice","finance_invoices","invoice_number","updated_at"],
    ["journal","finance_journals","journal_number","created_at"],
    ["payroll","hr_payroll_runs","pay_date","updated_at"],
  ] as const;
  for (const [kind,table,title,updated] of sources) {
    const loaders = {
      order: async () => (await import("./commerce")).workOrderRows(actor),
      invoice: async () => (await import("./finance")).workInvoiceRows(actor),
      journal: async () => (await import("./ledger")).workJournalRows(actor),
      payroll: async () => (await import("./people-payroll")).workPayrollRows(actor),
    };
    let rows: Record<string,string|number|null>[];
    try { rows = reference ? (await write.prepare(`SELECT * FROM ${table} WHERE company_id=? ORDER BY ${updated} DESC`).bind(actor.companyId).all<Record<string,string|number|null>>()).results : await loaders[kind]();
    } catch (error) { if (error instanceof ApiError && [403,404].includes(error.status)) continue; throw error; }
    await Promise.all(rows.map(row => policyRead(write, JSON.stringify(["flow-record",actor.companyId,kind,String(row.id)]), async () => row)));
    for (const row of rows) {
      if (row.archived_at || kind === "invoice" && row.source_type === "expense_claim") continue;
      try { if (reference) await requireFlowAccess(actor,kind,String(row.id)); } catch (error) { if (error instanceof ApiError && [403,404].includes(error.status)) continue; throw error; }
      const flow = await resolveFlow(write,actor,kind,String(row.id));
      const own = row.created_by === actor.userId;
      const participant = flow.steps.some(s => s.primary?.userId === actor.userId) || flow.history.some(e => e.actorName === actor.userId);
      if (!own && !participant && !(flow.canManage && flow.blocker)) continue;
      const href = kind === "order" ? `/commerce/orders/${row.id}` : kind === "invoice" ? `/finance/invoices/${row.id}` : kind === "journal" ? `/finance/journals/${row.id}` : `/people/payroll/${row.id}`;
      items.push({id:String(row.id),type:kind,title:String(row[title]),detail:flow.current?.action ?? "No pending action",href,status:flow.state,updatedAt:Number(row[updated]),queue:flow.canAct ? "todo" : flow.current ? "waiting" : "completed",flow,canAct:flow.canAct,category:flow.blocker && !flow.current?.primary ? "setup" : "operational",setupReason:flow.blocker,relationships:[...(own ? ["requested" as const] : []),...(flow.canAct ? ["assigned" as const] : [])]});
    }
  }
  return items;
}
