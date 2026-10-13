import type {WorkContext} from "../../identity_access/public/authorization";
import type {WorkItem} from "@/lib/work_projection/work-item";
import {mergeWorkItems,type WorkSnapshot} from "../domain/work-items";
import type {getWork} from "../public/work";
export type WorkSource="financeExpansionWork"|"legalWork"|"organizationChangeWork"|"companyBusinessAcceptanceWork"|"posWork"|"crmWork"|"supportAlertWork"|"incidentWork"|"supportPolicyWork"|"supportWork"|"supportSessionWork"|"maintenanceWork"|"preventivePlanWork"|"projectHandoffWork"|"fieldServiceWork"|"collaborationActionWork"|"operationsExceptionWork"|"learningWork"|"benefitWork"|"talentWork"|"marketingWork"|"manufacturingWork"|"customerAccessWork"|"customerDuplicateWork"|"customerIdentityWork"|"customerFollowupWork"|"supplierBillWork"|"landedCostWork"|"rfqWork"|"supplierPromiseWork"|"transitWork"|"qualityWork"|"qualityAlertWork"|"supplierBatchWork"|"refundWork"|"overtimeWork"|"pmsWork"|"employeeRequestWork"|"departmentOverrideWork"|"retirementWork"|"peopleLifecycleWork"|"otherFlowWork";
export type WorkSnapshotPorts={source(name:WorkSource,actor:WorkContext):Promise<WorkItem[]>;domain(actor:WorkContext,source:"expense"|"stock_count"|"transfer"|"staff_task"):Promise<WorkItem[]>;leave(actor:WorkContext):ReturnType<typeof getWork>;enrich(actor:WorkContext,items:WorkItem[]):Promise<WorkItem[]>;enabledModules(actor:WorkContext):Promise<Set<string>>;production():boolean;monotonic():number;performanceLog():boolean;errorStatus(error:unknown):number|undefined;info(value:string):void;warn(...values:unknown[]):void;cached(actor:WorkContext,load:()=>Promise<WorkSnapshot>):Promise<WorkSnapshot>};
export function createWorkSnapshot(ports:WorkSnapshotPorts){
/** Unmemoized reference reader for parity checks and cache fills. */
async function readWorkSnapshot(actor: WorkContext): Promise<WorkSnapshot> {
  // A composite queue skips inaccessible sources. Keep the same authority and
  // scope, but return domain 403s instead of interrupting the entire Next page.
  actor = { ...actor, pageRequest: undefined };
  const jobs: { label: string; load: () => Promise<WorkItem[]> }[] = [];
  if(!actor.beta)jobs.push({label:"Finance expansion reviews",load:async()=>ports.source("financeExpansionWork",actor)});
  jobs.push({label:"Legal",load:async()=> ports.source("legalWork",actor)});
  if (!actor.beta) jobs.push({label:"Overtime",load:()=>ports.source("overtimeWork",actor)});
  if (!actor.beta) jobs.push({label:"Hotel operations",load:()=>ports.source("pmsWork",actor)});
  // Each domain adapter resolves a complete applicable grant from all responsibilities.
  // Load each source once; repeating the same query for every role adds latency.
  const context = actor;
  {
    for (const source of [
      "expense",
      "stock_count",
      "transfer",
      "staff_task",
    ] as const) {
      if (actor.beta && ["stock_count", "transfer"].includes(source)) continue;
      if (source === "staff_task" && jobs.some((j) => j.label === "Tasks"))
        continue;
      const label = {
        expense: "Expense claims",
        stock_count: "Stock counts",
        transfer: "Warehouse transfers",
        staff_task: "Tasks",
      }[source];
      jobs.push({ label, load: () => ports.domain(context,source) });
    }
    if (!actor.beta && !jobs.some((j) => j.label === "Leave requests"))
      jobs.push({
        label: "Leave requests",
        load: async () => {
          const leave = await ports.leave(context);
          return leave.rows.map((row) => ({
            id: row.id,
            type: "leave" as const,
            title: `Leave request · ${row.number}`,
            detail:
              row.blockedReason ??
              (row.currentApprover
                ? `Awaiting ${row.currentApprover}`
                : `${row.start_date} · ${row.days} days · ${row.handover_note}`),
            href: `/tasks/people_payroll/${row.id}`,
            status: row.state,
            updatedAt: row.updated_at,
            queue: row.queue,
            actionKind: row.isApproval ? "approval" as const : "work" as const,
            responsibleQueue: row.currentApprover ?? "Requester",
            blocker: row.blockedReason,
            relationships: [
              ...(row.created_by === actor.userId
                ? ["requested" as const]
                : []),
              ...(row.isApproval ? ["approval" as const] : []),
            ],
          }));
        },
      });
  }
  jobs.push({
    label: "Employee requests",
    load: async () =>
      ports.source("employeeRequestWork",actor),
  });
  jobs.push({label:"Department overrides",load:()=>ports.source("departmentOverrideWork",actor)});
  if (!actor.beta) jobs.push({label:"Retirement clearance",load:async()=>ports.source("retirementWork",actor)});
  if (!actor.beta) jobs.push({label:"People & IT preparation",load:async()=>ports.source("peopleLifecycleWork",actor)});
  if (!actor.beta) jobs.push({label:"Organization changes",load:async()=>ports.source("organizationChangeWork",actor)});
  if (!actor.beta) jobs.push({label:"Company setup acceptance",load:async()=>ports.source("companyBusinessAcceptanceWork",actor)});
  if (!actor.beta) jobs.push({label:"POS",load:async()=>ports.source("posWork",actor)});
  if (!actor.beta) jobs.push({label:"CRM pipeline",load:async()=>ports.source("crmWork",actor)});
  if (!actor.beta) jobs.push({label:"Support target escalations",load:async()=>ports.source("supportAlertWork",actor)});
  if (!actor.beta) jobs.push({label:"Support incidents",load:async()=>ports.source("incidentWork",actor)});
  if (!actor.beta) jobs.push({label:"Support SLA policies",load:async()=>ports.source("supportPolicyWork",actor)});
  if (!actor.beta) jobs.push({label:"Support tickets",load:async()=>ports.source("supportWork",actor)});
  if (!actor.beta) jobs.push({label:"Temporary support access",load:async()=>ports.source("supportSessionWork",actor)});
  if (!actor.beta) jobs.push({label:"Maintenance work",load:async()=>ports.source("maintenanceWork",actor)});
  if (!actor.beta) jobs.push({label:"Preventive maintenance",load:async()=>ports.source("preventivePlanWork",actor)});
  if (!actor.beta) jobs.push({label:"Project handoffs",load:async()=>ports.source("projectHandoffWork",actor)});
  if (!actor.beta) jobs.push({label:"Field service",load:async()=>ports.source("fieldServiceWork",actor)});
  if (!actor.beta) jobs.push({label:"Cross-record actions",load:async()=>ports.source("collaborationActionWork",actor)});
  if (!actor.beta) jobs.push({label:"Operational exceptions",load:async()=>ports.source("operationsExceptionWork",actor)});
  if (!actor.beta) jobs.push({label:"Learning assignments",load:async()=>ports.source("learningWork",actor)});
  if (!actor.beta) jobs.push({label:"Benefit enrollments",load:async()=>ports.source("benefitWork",actor)});
  if (!actor.beta) jobs.push({label:"Talent applications",load:async()=>ports.source("talentWork",actor)});
  if (!actor.beta) jobs.push({label:"Marketing lead candidates",load:async()=>ports.source("marketingWork",actor)});
  if (!actor.beta) jobs.push({label:"Manufacturing orders",load:async()=>ports.source("manufacturingWork",actor)});
  if (!actor.beta) jobs.push({label:"Customer access reviews",load:async()=>ports.source("customerAccessWork",actor)});
  if (!actor.beta) jobs.push({label:"Customer duplicate reviews",load:async()=>ports.source("customerDuplicateWork",actor)});
  if (!actor.beta) jobs.push({label:"Customer identity changes",load:async()=>ports.source("customerIdentityWork",actor)});
  if (!actor.beta) jobs.push({label:"Customer follow-ups",load:async()=>ports.source("customerFollowupWork",actor)});
  if (!actor.beta) jobs.push({label:"Supplier invoice reviews",load:async()=>ports.source("supplierBillWork",actor)});
  if (!actor.beta) jobs.push({label:"Inventory landed costs",load:async()=>ports.source("landedCostWork",actor)});
  if (!actor.beta) jobs.push({label:"Supplier quotation reviews",load:async()=>ports.source("rfqWork",actor)});
  if (!actor.beta) jobs.push({label:"Supplier confirmations",load:async()=>ports.source("supplierPromiseWork",actor)});
  if (!actor.beta) jobs.push({label:"Stock in transit",load:async()=>ports.source("transitWork",actor)});
  if (!actor.beta) jobs.push({label:"Quality holds",load:async()=>ports.source("qualityWork",actor)});
  if (!actor.beta) jobs.push({label:"Quality escalations",load:async()=>ports.source("qualityAlertWork",actor)});
  if (!actor.beta) jobs.push({label:"Supplier payment batches",load:async()=>ports.source("supplierBatchWork",actor)});
  if (!actor.beta) jobs.push({label:"Return refunds",load:async()=>ports.source("refundWork",actor)});
  jobs.push({label:"Commercial and financial workflows",load:() => ports.source("otherFlowWork",actor)});
  if (ports.production()) {
    const enabled = await ports.enabledModules(actor);
    const sources: Record<string,string[]> = {
      Legal:['legal'], Overtime:['people_payroll'], 'Hotel operations':['pms'],
      'Expense claims':['people_payroll'], 'Stock counts':['inventory'], 'Warehouse transfers':['inventory'],
      Tasks:['operations'], 'Leave requests':['people_payroll'], 'Employee requests':['people_payroll'],
      'Retirement clearance':['people_payroll'], 'People & IT preparation':['identity_access'],
      'Organization changes':['organization'], 'Company setup acceptance':['organization'], POS:['pos'],
      'CRM pipeline':['crm'], 'Support target escalations':['support'], 'Support incidents':['support'],
      'Support SLA policies':['support'], 'Support tickets':['support'], 'Temporary support access':['support'],
      'Maintenance work':['maintenance'], 'Preventive maintenance':['maintenance'], 'Field service':['field_service'],
      'Project handoffs':['projects'],
      'Cross-record actions':['collaboration'], 'Learning assignments':['learning'],
      'Operational exceptions':['operations'],
      'Benefit enrollments':['benefits'], 'Talent applications':['talent'],
      'Marketing lead candidates':['marketing'], 'Manufacturing orders':['manufacturing'],
      'Customer access reviews':['customer_authority'], 'Customer duplicate reviews':['customer_authority'],
      'Customer identity changes':['customer_authority'], 'Customer follow-ups':['crm'],
      'Supplier invoice reviews':['procurement'], 'Inventory landed costs':['inventory'],
      'Supplier quotation reviews':['procurement'], 'Supplier confirmations':['procurement'],
      'Stock in transit':['inventory'], 'Quality holds':['quality'], 'Quality escalations':['quality'],
      'Supplier payment batches':['procurement'], 'Return refunds':['sales'],
      'Commercial and financial workflows':['finance'],
      'Finance expansion reviews':['finance'],
    };
    for (let index=jobs.length-1;index>=0;index--)
      if (!(sources[jobs[index].label]??[]).some((moduleId)=>enabled.has(moduleId))) jobs.splice(index,1);
  }
  const settled = await Promise.allSettled(jobs.map(async (j) => {
    const started = ports.monotonic();
    try { return await ports.enrich(actor,await j.load()); }
    finally { if (ports.performanceLog()) ports.info(JSON.stringify({ event: "work_source", source: j.label, durationMs: ports.monotonic() - started })); }
  })),
    rows: WorkItem[] = [],
    unavailable = new Set<string>();
  settled.forEach((result, i) => {
    if (result.status === "fulfilled") rows.push(...result.value);
    else if (
      !(
        [403,404].includes(ports.errorStatus(result.reason)??0)
      )
    ) {
      unavailable.add(jobs[i].label);
      ports.warn("work_source_unavailable", {
        source: jobs[i].label,
        error: result.reason instanceof Error ? result.reason.name : "Error",
        message:
          result.reason instanceof Error
            ? result.reason.message
            : "Unknown work source failure",
        code:
          typeof result.reason === "object" &&
          result.reason !== null &&
          "code" in result.reason
            ? String(result.reason.code)
            : undefined,
      });
    }
  });
  return { items: mergeWorkItems(rows), unavailableSources: [...unavailable] };
}
async function workItems(actor: WorkContext) {
  return (await ports.cached(actor,()=>readWorkSnapshot(actor))).items;
}

return {readWorkSnapshot,workItems};}
