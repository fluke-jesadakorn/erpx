import { cachedWorkSnapshot } from "./work-cache";
import { measureReads } from "./runtime/read-metrics";
import { overtimeWork } from "./overtime";
import { retirementWork } from "./retirement-clearance";
import { peopleLifecycleWork } from "./people-lifecycle";
import { pmsWork } from "./pms";
import {withPolicyReadSnapshot} from "./iam-read-snapshot";
import { enrichFlowWork, otherFlowWork } from "./flow-work";
import "server-only";
import type { WorkContext } from "./authorization";
import { getDomainWork } from "./domain-work";
import { getWork } from "./work";
import { compareWork, isMyAction, type WorkItem } from "@/lib/work-item";

import { holdingResources } from "./runtime";
import { employeeRequestWork } from "./employee-requests";
import { ApiError } from "./http";

export type WorkSnapshot = { items: WorkItem[]; unavailableSources: string[] };

export function mergeWorkItems(rows: WorkItem[]) {
  const merged = new Map<string, WorkItem>();
  for (const item of rows) {
    const key = `${item.type}:${item.id}`,
      old = merged.get(key);
    if (!old) {
      merged.set(key, { ...item, nextAction: item.nextAction ?? item.detail });
      continue;
    }
    const actionable =
      old.queue === "todo"
        ? old
        : item.queue === "todo"
          ? item
          : old.updatedAt >= item.updatedAt
            ? old
            : item;
    merged.set(key, {
      ...actionable,
      nextAction: actionable.nextAction ?? actionable.detail,
      relationships: [
        ...new Set([
          ...(old.relationships ?? []),
          ...(item.relationships ?? []),
        ]),
      ],
    });
  }
  return [...merged.values()].sort(compareWork);
}
export function nextWorkSteps(items: WorkItem[], now = Date.now()) {
  return items
    .filter(isMyAction)
    .sort((a, b) => {
      const overdueA = a.dueAt && a.dueAt < now ? 1 : 0,
        overdueB = b.dueAt && b.dueAt < now ? 1 : 0;
      return (
        overdueB - overdueA ||
        (a.dueAt ?? Infinity) - (b.dueAt ?? Infinity) ||
        b.updatedAt - a.updatedAt ||
        a.id.localeCompare(b.id)
      );
    })
    .slice(0, 3);
}
export async function workSnapshot(actor: WorkContext): Promise<WorkSnapshot> {
  const { write } = await holdingResources(actor.holdingId);
  return cachedWorkSnapshot(actor, write, async () => {
    const measured = await measureReads(() => withPolicyReadSnapshot(() => readWorkSnapshot(actor)));
    if (process.env.ERPX_PERF_LOG === "1") console.info(JSON.stringify({ event: "work_snapshot", ...measured.metrics, items: measured.result.items.length }));
    return measured.result;
  });
}
/** Unmemoized reference reader for parity checks and cache fills. */
export async function readWorkSnapshot(actor: WorkContext): Promise<WorkSnapshot> {
  const jobs: { label: string; load: () => Promise<WorkItem[]> }[] = [];
  jobs.push({label:"Legal",load:async()=> (await import("./legal/integrations")).legalWork((await holdingResources(actor.holdingId)).write,actor)});
  if (!actor.beta) jobs.push({label:"Overtime",load:()=>overtimeWork(actor)});
  if (!actor.beta) jobs.push({label:"Hotel operations",load:()=>pmsWork(actor)});
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
      jobs.push({ label, load: () => getDomainWork(context, source) });
    }
    if (!actor.beta && !jobs.some((j) => j.label === "Leave requests"))
      jobs.push({
        label: "Leave requests",
        load: async () => {
          const leave = await getWork(actor.holdingId, context);
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
      employeeRequestWork(
        (await holdingResources(actor.holdingId)).write,
        actor,
      ),
  });
  if (!actor.beta) jobs.push({label:"Retirement clearance",load:async()=>retirementWork((await holdingResources(actor.holdingId)).write,actor)});
  if (!actor.beta) jobs.push({label:"People & IT preparation",load:async()=>peopleLifecycleWork((await holdingResources(actor.holdingId)).write,actor)});
  if (!actor.beta) jobs.push({label:"Organization changes",load:async()=>(await import("./organization-work")).organizationChangeWork((await holdingResources(actor.holdingId)).write,actor)});
  if (!actor.beta) jobs.push({label:"Company setup acceptance",load:async()=>(await import("./company-business-acceptance")).companyBusinessAcceptanceWork((await holdingResources(actor.holdingId)).write,actor)});
  if (!actor.beta) jobs.push({label:"POS",load:async()=>(await import("./pos")).posWork(actor)});
  if (!actor.beta) jobs.push({label:"CRM pipeline",load:async()=>(await import("./crm")).crmWork(actor)});
  if (!actor.beta) jobs.push({label:"Support target escalations",load:async()=>(await import("./support-sla-alerts")).supportAlertWork(actor)});
  if (!actor.beta) jobs.push({label:"Support incidents",load:async()=>(await import("./support-incidents")).incidentWork(actor)});
  if (!actor.beta) jobs.push({label:"Support SLA policies",load:async()=>(await import("./support-sla")).supportPolicyWork(actor)});
  if (!actor.beta) jobs.push({label:"Support tickets",load:async()=>(await import("./support")).supportWork(actor)});
  if (!actor.beta) jobs.push({label:"Temporary support access",load:async()=>(await import("./support-sessions")).supportSessionWork(actor)});
  if (!actor.beta) jobs.push({label:"Maintenance work",load:async()=>(await import("./asset-maintenance")).maintenanceWork(actor)});
  if (!actor.beta) jobs.push({label:"Field service",load:async()=>(await import("./field-service")).fieldServiceWork(actor)});
  if (!actor.beta) jobs.push({label:"Cross-record actions",load:async()=>(await import("./collaboration-work")).collaborationActionWork(actor)});
  if (!actor.beta) jobs.push({label:"Learning assignments",load:async()=>(await import("./learning")).learningWork(actor)});
  if (!actor.beta) jobs.push({label:"Benefit enrollments",load:async()=>(await import("./benefits")).benefitWork(actor)});
  if (!actor.beta) jobs.push({label:"Talent applications",load:async()=>(await import("./talent")).talentWork(actor)});
  if (!actor.beta) jobs.push({label:"Marketing lead candidates",load:async()=>(await import("./marketing")).marketingWork(actor)});
  if (!actor.beta) jobs.push({label:"Manufacturing orders",load:async()=>(await import("./production")).manufacturingWork(actor)});
  if (!actor.beta) jobs.push({label:"Customer access reviews",load:async()=>(await import("./customer-access")).customerAccessWork(actor)});
  if (!actor.beta) jobs.push({label:"Customer duplicate reviews",load:async()=>(await import("./customer-duplicates")).customerDuplicateWork(actor)});
  if (!actor.beta) jobs.push({label:"Customer identity changes",load:async()=>(await import("./customer-identity-changes")).customerIdentityWork(actor)});
  if (!actor.beta) jobs.push({label:"Customer follow-ups",load:async()=>(await import("./customer-followups")).customerFollowupWork(actor)});
  if (!actor.beta) jobs.push({label:"Supplier invoice reviews",load:async()=>(await import("./supplier-bill-matching")).supplierBillWork(actor)});
  if (!actor.beta) jobs.push({label:"Inventory landed costs",load:async()=>(await import("./inventory-landed-costs")).landedCostWork(actor)});
  if (!actor.beta) jobs.push({label:"Supplier quotation reviews",load:async()=>(await import("./procurement-rfqs")).rfqWork(actor)});
  if (!actor.beta) jobs.push({label:"Supplier confirmations",load:async()=>(await import("./supplier-promises")).supplierPromiseWork(actor)});
  if (!actor.beta) jobs.push({label:"Stock in transit",load:async()=>(await import("./inventory-transit")).transitWork(actor)});
  if (!actor.beta) jobs.push({label:"Quality holds",load:async()=>(await import("./quality")).qualityWork(actor)});
  if (!actor.beta) jobs.push({label:"Supplier payment batches",load:async()=>(await import("./supplier-payments")).supplierBatchWork(actor)});
  if (!actor.beta) jobs.push({label:"Return refunds",load:async()=>(await import("./refunds")).refundWork(actor)});
  jobs.push({label:"Commercial and financial workflows",load:() => otherFlowWork(actor)});
  const settled = await Promise.allSettled(jobs.map(async (j) => {
    const started = performance.now();
    try { return await enrichFlowWork(actor, await j.load()); }
    finally { if (process.env.ERPX_PERF_LOG === "1") console.info(JSON.stringify({ event: "work_source", source: j.label, durationMs: performance.now() - started })); }
  })),
    rows: WorkItem[] = [],
    unavailable = new Set<string>();
  settled.forEach((result, i) => {
    if (result.status === "fulfilled") rows.push(...result.value);
    else if (
      !(
        result.reason instanceof ApiError &&
        [403, 404].includes(result.reason.status)
      )
    ) {
      unavailable.add(jobs[i].label);
      console.warn("work_source_unavailable", {
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
export async function workItems(actor: WorkContext) {
  return (await workSnapshot(actor)).items;
}
