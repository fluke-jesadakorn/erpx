import {withPolicyReadSnapshot} from "./iam-read-snapshot";
import { enrichFlowWork, otherFlowWork } from "./flow-work";
import "server-only";
import type { WorkContext } from "./authorization";
import { getDomainWork } from "./domain-work";
import { getWork } from "./work";
import type { WorkItem } from "@/lib/work-item";

import { holdingResources } from "./runtime";
import { employeeRequestWork } from "./employee-requests";
import { ApiError } from "./http";

export type WorkSnapshot = { items: WorkItem[]; unavailableSources: string[] };
const activeSnapshots = new Map<string, Promise<WorkSnapshot>>();

function snapshotKey(actor: WorkContext) {
  return [
    actor.holdingId,
    actor.companyId,
    actor.userId,
    actor.assignmentId,
    actor.authority?.revision ?? 0,
    actor.scope.kind,
    actor.branchId ?? "",
    actor.departmentId ?? "",
  ].join(":");
}
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
  return [...merged.values()].sort(
    (a, b) =>
      b.updatedAt - a.updatedAt ||
      `${a.type}:${a.id}`.localeCompare(`${b.type}:${b.id}`),
  );
}
export function nextWorkSteps(items: WorkItem[], now = Date.now()) {
  return items
    .filter((i) => i.queue === "todo")
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
export async function workSnapshot(actor:WorkContext):Promise<WorkSnapshot>{
 const key = snapshotKey(actor);
 const existing = activeSnapshots.get(key);
 if (existing) return existing;
 const snapshot = withPolicyReadSnapshot(()=>readWorkSnapshot(actor));
 activeSnapshots.set(key, snapshot);
 void snapshot.finally(() => {
   if (activeSnapshots.get(key) === snapshot) activeSnapshots.delete(key);
 }).catch(() => {});
 return snapshot;
}
async function readWorkSnapshot(actor: WorkContext): Promise<WorkSnapshot> {
  const jobs: { label: string; load: () => Promise<WorkItem[]> }[] = [];
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
            title: `Annual leave request · ${row.number}`,
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
  jobs.push({label:"Commercial and financial workflows",load:() => otherFlowWork(actor)});
  const settled = await Promise.allSettled(jobs.map(async (j) => enrichFlowWork(actor,await j.load()))),
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
      console.error("work_source_unavailable", {
        source: jobs[i].label,
        error: result.reason instanceof Error ? result.reason.name : "Error",
      });
    }
  });
  return { items: mergeWorkItems(rows), unavailableSources: [...unavailable] };
}
export async function workItems(actor: WorkContext) {
  return (await workSnapshot(actor)).items;
}
