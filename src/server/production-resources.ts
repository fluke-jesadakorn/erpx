import "server-only";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import type { Row } from "./commerce";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import { holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { hasModuleAccess } from "@/data/module-access";
import { assertion, executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";

const resourceSchema = z.object({
  expectedVersion: z.number().int().positive().optional(), branchId: z.string().min(1),
  code: z.string().trim().min(2).max(64), name: z.string().trim().min(2).max(160),
  dailyMinutes: z.number().int().min(1).max(1440),
  workWeekdays: z.array(z.number().int().min(0).max(6)).min(1).max(7).default([1, 2, 3, 4, 5]),
  status: z.enum(["active", "archived"]).default("active"),
});
const calendarSchema = z.object({
  expectedVersion: z.number().int().nonnegative(), resourceVersion: z.number().int().positive(),
  workDate: z.string().date(), availableMinutes: z.number().int().min(0).max(1440),
  reason: z.string().trim().min(3).max(3000),
});
function access(a: WorkContext) {
  requirePermission(a, "records.write");
  requirePermission(a, "records.review");
  if (!hasModuleAccess(a.modules, "planning") || a.departmentId || a.beta)
    throw new ApiError(403, "PLANNING_RESOURCE_SCOPE", "Choose a Company or Branch planning reviewer to configure resources.");
}
export async function saveProductionResource(a: WorkContext, id: string | null, raw: unknown, key: string) {
  access(a);
  const i = resourceSchema.parse(raw), { write } = await holdingResources(a.holdingId), resourceId = id ?? `production-resource-${key}`,
    op = id ? "planning.resource.update" : "planning.resource.create", prior = await replayCommand(write, a, key, op, resourceId, i);
  if (prior) return prior;
  if (a.branchId && i.branchId !== a.branchId) throw new ApiError(403, "PLANNING_RESOURCE_SCOPE", "Configure resources only in the current branch.");
  const now = Date.now(), weekdays = [...new Set(i.workWeekdays)].sort(), steps: SqlStatement[] = [
    assertion(write, key, "branch", "EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind IN ('branch','headquarters') AND status='active')", [i.branchId, a.companyId]),
  ];
  if (id) {
    if (!i.expectedVersion) throw new ApiError(422, "VERSION_REQUIRED", "Use the current resource version.");
    steps.push(
      assertion(write, key, "resource", "EXISTS(SELECT 1 FROM production_resources WHERE id=? AND company_id=? AND branch_id=? AND version=?)", [id, a.companyId, i.branchId, i.expectedVersion]),
      assertion(write, key, "active-resource-allocation", "NOT EXISTS(SELECT 1 FROM planning_resource_allocations WHERE resource_id=? AND released_at IS NULL)", [id]),
      write.prepare("UPDATE production_resources SET code=?,name=?,daily_minutes=?,work_weekdays_json=?,status=?,version=version+1,updated_at=? WHERE id=?")
        .bind(i.code, i.name, i.dailyMinutes, JSON.stringify(weekdays), i.status, now, id),
    );
  } else steps.push(write.prepare(`INSERT INTO production_resources(id,company_id,branch_id,code,name,daily_minutes,work_weekdays_json,status,version,created_by,created_at,updated_at)
    VALUES(?,?,?,?,?,?,?,?,1,?,?,?)`).bind(resourceId, a.companyId, i.branchId, i.code, i.name, i.dailyMinutes, JSON.stringify(weekdays), i.status, a.userId, now, now));
  return executeCommand(write, a, key, op, i, resourceId, (i.expectedVersion ?? 0) + 1, { id: resourceId, version: (i.expectedVersion ?? 0) + 1, status: i.status }, steps);
}
export async function saveProductionResourceCalendar(a: WorkContext, id: string, raw: unknown, key: string) {
  access(a);
  const i = calendarSchema.parse(raw), { write } = await holdingResources(a.holdingId), op = "planning.resource.calendar",
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  return executeCommand(write, a, key, op, i, id, i.expectedVersion + 1, { id, workDate: i.workDate, version: i.expectedVersion + 1 }, [
    write.prepare("SELECT id FROM production_resources WHERE id=? FOR UPDATE").bind(id),
    assertion(write, key, "resource", "EXISTS(SELECT 1 FROM production_resources WHERE id=? AND company_id=? AND status='active' AND version=? AND (?::text IS NULL OR branch_id=?))", [id, a.companyId, i.resourceVersion, a.branchId, a.branchId]),
    assertion(write, key, "calendar-version", "COALESCE((SELECT version FROM production_resource_calendar WHERE resource_id=? AND work_date=?),0)=?", [id, i.workDate, i.expectedVersion]),
    assertion(write, key, "released-capacity", "COALESCE((SELECT sum(minutes) FROM planning_resource_allocations WHERE resource_id=? AND work_date=? AND released_at IS NULL),0)<=?", [id, i.workDate, i.availableMinutes]),
    write.prepare(`INSERT INTO production_resource_calendar(resource_id,work_date,available_minutes,reason,version,updated_by,updated_at)
      VALUES(?,?,?,?,1,?,?) ON CONFLICT(resource_id,work_date) DO UPDATE SET available_minutes=excluded.available_minutes,reason=excluded.reason,version=production_resource_calendar.version+1,updated_by=excluded.updated_by,updated_at=excluded.updated_at`)
      .bind(id, i.workDate, i.availableMinutes, i.reason, a.userId, Date.now()),
  ]);
}

export type ResourceAllocation = { resourceId: string; workDate: string; minutes: number; resourceVersion: number; calendarVersion: number; availableMinutes: number };
export type PlanningContext = {
  demandSource?: { id: string; version: number; quantityMilli: number };
  incomingSupply?: Array<{ id: string; version: number; productId: string; quantityMilli: number; dueDate: string }>;
  stock?: Array<{ productId: string; version: number }>;
  allocations?: ResourceAllocation[];
  operations?: Array<{ id: string; name: string; resourceId: string; requiredMinutes: number }>;
  startDate?: string; capacityBasis?: string; comparisonScenarioId?: string | null;
  comparison?: { scenarioId: string; sourceVersion: number; shortageDeltaMilli: number; requiredMinutesDelta: number; feasibleDate: string; currentFeasibleDate: string };
};
const addDay = (date: string) => new Date(Date.parse(`${date}T00:00:00Z`) + 86400000).toISOString().slice(0, 10);

export async function calculateProductionCapacity(db: SqlDatabase, a: WorkContext, input: { specificationId: string; branchId: string; factor: number; startDate: string; dueDate: string }) {
  const operations = (await db.prepare(`SELECT o.*,r.version resource_version,r.status,r.daily_minutes,r.work_weekdays_json,r.branch_id
    FROM manufacturing_specification_operations o JOIN production_resources r ON r.id=o.resource_id
    WHERE o.specification_id=? AND r.company_id=? ORDER BY o.ordinal`)
    .bind(input.specificationId, a.companyId).all<Row>()).results;
  if (!operations.length) return null;
  let cursor = input.startDate, requiredMinutes = 0;
  const allocations: ResourceAllocation[] = [], sourceOperations: NonNullable<PlanningContext["operations"]> = [];
  for (const operation of operations) {
    if (operation.status !== "active" || operation.branch_id !== input.branchId)
      throw new ApiError(409, "PLANNING_RESOURCE_CHANGED", "An approved routing resource is inactive or belongs to another branch.");
    let remaining = Number(operation.setup_minutes) + Math.ceil(Number(operation.run_minutes_per_batch) * input.factor);
    requiredMinutes += remaining;
    sourceOperations.push({ id: String(operation.id), name: String(operation.name), resourceId: String(operation.resource_id), requiredMinutes: remaining });
    const weekdays = JSON.parse(String(operation.work_weekdays_json)) as number[];
    for (let day = 0; remaining > 0; day++) {
      if (day > 366) throw new ApiError(422, "PLANNING_CAPACITY_UNAVAILABLE", "The routing cannot be scheduled within a year using the configured resource calendar.");
      const calendar = await db.prepare("SELECT available_minutes,version FROM production_resource_calendar WHERE resource_id=? AND work_date=?")
        .bind(operation.resource_id, cursor).first<{ available_minutes: number; version: number }>(),
        existing = await db.prepare("SELECT COALESCE(sum(minutes),0) minutes FROM planning_resource_allocations WHERE resource_id=? AND work_date=? AND released_at IS NULL")
          .bind(operation.resource_id, cursor).first<{ minutes: number }>(),
        available = calendar ? Number(calendar.available_minutes) : weekdays.includes(new Date(`${cursor}T00:00:00Z`).getUTCDay()) ? Number(operation.daily_minutes) : 0,
        used = allocations.filter(x => x.resourceId === operation.resource_id && x.workDate === cursor).reduce((n, x) => n + x.minutes, 0),
        minutes = Math.min(remaining, Math.max(0, available - Number(existing?.minutes ?? 0) - used));
      if (minutes > 0) allocations.push({ resourceId: String(operation.resource_id), workDate: cursor, minutes,
        resourceVersion: Number(operation.resource_version), calendarVersion: Number(calendar?.version ?? 0), availableMinutes: available });
      remaining -= minutes;
      if (remaining > 0) cursor = addDay(cursor);
    }
  }
  // Consolidate a resource reused by consecutive routing operations on one day.
  const grouped = new Map<string, ResourceAllocation>();
  for (const allocation of allocations) {
    const key = `${allocation.resourceId}:${allocation.workDate}`, found = grouped.get(key);
    if (found) found.minutes += allocation.minutes; else grouped.set(key, { ...allocation });
  }
  const consolidated = [...grouped.values()];
  return { requiredMinutes, feasibleDate: cursor,
    capacityMinutes: consolidated.filter(x => x.workDate <= input.dueDate).reduce((n, x) => n + x.minutes, 0),
    allocations: consolidated, operations: sourceOperations };
}

export function planningContextGuards(db: SqlDatabase, a: WorkContext, key: string, context: PlanningContext): SqlStatement[] {
  const guards: SqlStatement[] = [];
  for (const [n, source] of [context.demandSource, ...(context.incomingSupply ?? [])].filter((x): x is NonNullable<typeof x> => Boolean(x)).entries()) {
    guards.push(db.prepare("SELECT id FROM commerce_orders WHERE id=? FOR SHARE").bind(source.id),
      assertion(db, key, `planning-source-${n}`, "EXISTS(SELECT 1 FROM commerce_orders WHERE id=? AND company_id=? AND version=? AND state IN ('confirmed','partially_fulfilled'))", [source.id, a.companyId, source.version]));
  }
  for (const [n, allocation] of (context.allocations ?? []).entries()) {
    guards.push(db.prepare("SELECT id FROM production_resources WHERE id=? FOR UPDATE").bind(allocation.resourceId),
      assertion(db, key, `planning-resource-${n}`, "EXISTS(SELECT 1 FROM production_resources WHERE id=? AND company_id=? AND version=? AND status='active')", [allocation.resourceId, a.companyId, allocation.resourceVersion]),
      assertion(db, key, `planning-calendar-${n}`, "COALESCE((SELECT version FROM production_resource_calendar WHERE resource_id=? AND work_date=?),0)=?", [allocation.resourceId, allocation.workDate, allocation.calendarVersion]));
  }
  return guards;
}
