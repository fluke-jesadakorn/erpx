import { preventivePlanSchema, preventiveActionSchema, type Plan, type PreventivePlanRow, type PreventiveInput, type PreventiveAction } from "@erpx/server-maintenance/domain/preventive-plan";
import { wallClock, nextOccurrence } from "@erpx/server-maintenance/domain/recurrence";
import { localDateTimeCandidates } from "@erpx/feature-work_projection/lib/calendar-timeline";
import type { WorkItem } from "@erpx/feature-work_projection/lib/work-item";
import { BusinessRuleError } from "@erpx/server-shared/index";
import type { Clock } from "@erpx/server-shared/application/ports";
import type { MaintenanceCreateInput } from "@erpx/server-maintenance/domain/work";

export type PreventiveCommand<Input = PreventiveAction> = { key: string; operation: string; id: string; input: Input };
export type PreventiveGeneration = { workOrderId: unknown; assetVersion: number; next: ReturnType<typeof nextOccurrence>; definition: Record<string, unknown> };
export type PreventiveRepository = {
  transaction?<Result>(action: (repository: PreventiveRepository) => Promise<Result>): Promise<Result>;
  replay(command: PreventiveCommand<PreventiveInput | PreventiveAction>): Promise<Record<string, unknown> | null>;
  plans(): Promise<PreventivePlanRow[]>; plan(id: string): Promise<Plan | null>; lockPlan(id: string): Promise<void>;
  assetVersion(id: string, lock: boolean): Promise<{ version: number } | null>;
  people(id: string): Promise<{ id: string; name: string }[]>; history(id: string): Promise<Record<string, unknown>[]>; occurrences(id: string): Promise<Record<string, unknown>[]>;
  save(command: PreventiveCommand<PreventiveInput>, current: Plan | null, facts: { local: string; dueAt: number; now: number; nextVersion: number }): Promise<Record<string, unknown>>;
  createWork(input: MaintenanceCreateInput, key: string): Promise<Record<string, unknown>>;
  change(command: PreventiveCommand, plan: Plan, facts: { now: number; status: string; result: Record<string, unknown>; generation?: PreventiveGeneration }): Promise<Record<string, unknown>>;
};
export function createPreventiveApplication(ports: {
  actorId: string; authorize(write?: boolean): void; authorizeReview(): void; canRead(): boolean; canWrite(): boolean; canReview(): boolean;
  transactionBound?: boolean;
  repository(): Promise<PreventiveRepository>; clock: Clock; error(status: number, code: string, message: string): Error;
}) {
  const fail = (status: number, code: string, message: string): never => { throw ports.error(status, code, message); };
  async function planFrom(repository: PreventiveRepository, id: string) {
    const plan = await repository.plan(id);
    if (!plan) return fail(404, "MAINTENANCE_PLAN_NOT_FOUND", "Preventive plan not found in this scope.");
    return plan;
  }
  async function preventivePlans() { ports.authorize(); return (await ports.repository()).plans(); }
  async function preventivePlanDetail(id: string) {
    ports.authorize(); const repository = await ports.repository(), plan = await planFrom(repository, id);
    return { plan, actorId: ports.actorId, canWrite: ports.canWrite(), canReview: ports.canReview(), assetVersion: Number((await repository.assetVersion(plan.asset_id, false))?.version ?? null), people: await repository.people(id), history: await repository.history(id), occurrences: await repository.occurrences(id) };
  }
  async function savePreventivePlan(id: string | null, raw: unknown, key: string) {
    ports.authorize(true); const input = preventivePlanSchema.parse(raw), repository = await ports.repository(), command = { key, operation: id ? "maintenance.plan.revise" : "maintenance.plan.create", id: id ?? `maintenance-plan-${key}`, input }, prior = await repository.replay(command);
    if (prior) return prior;
    const current = id ? await planFrom(repository, id) : null;
    if (current && (!["draft", "paused"].includes(current.status) || current.asset_id !== input.assetId)) return fail(409, "MAINTENANCE_PLAN_PAUSE_REQUIRED", "Pause the same asset plan before revising its definition.");
    if (current && current.created_by !== ports.actorId && !ports.canReview()) return fail(403, "MAINTENANCE_PLAN_REVIEW_REQUIRED", "The plan requester or an eligible reviewer must revise the definition.");
    const local = wallClock(input.firstDueAt, input.timeZone), dueAt = localDateTimeCandidates(local, input.timeZone)[0], now = ports.clock.now(), nextVersion = current ? Number(input.expectedVersion) + 1 : 1;
    if (current && !input.expectedVersion) return fail(422, "VERSION_REQUIRED", "Use the current plan version.");
    return repository.save(command, current, { local, dueAt, now, nextVersion });
  }
  async function changePreventivePlan(id: string, raw: unknown, key: string, current?: PreventiveRepository): Promise<Record<string, unknown>> {
    ports.authorize(true); const input = preventiveActionSchema.parse(raw), repository = current ?? await ports.repository();
    if (!current && !ports.transactionBound && input.action === "generate") {
      if (!repository.transaction) return fail(503, "MAINTENANCE_TRANSACTION_REQUIRED", "The transaction runtime is unavailable. Retry when the service is ready.");
      return repository.transaction(transaction => changePreventivePlan(id, input, key, transaction));
    }
    const command = { key, operation: `maintenance.plan.${input.action}`, id, input }, prior = await repository.replay(command);
    if (prior) return prior;
    if (input.action === "generate") await repository.lockPlan(id);
    const plan = await planFrom(repository, id), now = ports.clock.now(), nextVersion = input.expectedVersion + 1;
    if (plan.version !== input.expectedVersion) return fail(409, "CONFLICT", "The preventive plan changed. Reload it before continuing.");
    let status = plan.status, result: Record<string, unknown> = { id, version: nextVersion }, generation: PreventiveGeneration | undefined;
    if (input.action === "activate") {
      ports.authorizeReview();
      if (!["draft", "paused"].includes(plan.status)) return fail(409, "MAINTENANCE_PLAN_STATE", "Review a draft or paused plan.");
      if (plan.edited_by === ports.actorId) return fail(409, "INDEPENDENT_PLAN_REVIEW_REQUIRED", "A different reviewer must activate the current preventive definition.");
      status = "active";
    } else if (input.action === "generate") {
      if (plan.status !== "active") return fail(409, "MAINTENANCE_PLAN_INACTIVE", "Activate this plan before generating its due work.");
      if (plan.owner_id !== ports.actorId && !ports.canReview()) return fail(403, "MAINTENANCE_PLAN_OWNER_REQUIRED", "The assigned owner or a reviewer must generate due work.");
      if (plan.next_due_at > now + plan.lead_days * 86400000) return fail(409, "MAINTENANCE_PLAN_NOT_DUE", "This occurrence is outside the approved planning window.");
      let next: ReturnType<typeof nextOccurrence>;
      try { next = nextOccurrence(plan.next_due_local, plan.time_zone, plan.interval_days); }
      catch (failure) { if (failure instanceof BusinessRuleError) throw ports.error(failure.status, failure.code, failure.message); throw failure; }
      const asset = await repository.assetVersion(plan.asset_id, true);
      if (!asset) return fail(409, "MAINTENANCE_PLAN_SOURCE_CHANGED", "Reload the current asset before generating work.");
      const definition = { title: plan.title, priority: plan.priority, ownerId: plan.owner_id, intervalDays: plan.interval_days, timeZone: plan.time_zone, leadDays: plan.lead_days, dueLocal: plan.next_due_local, dueAt: plan.next_due_at, nextResolvedLocal: next.resolvedLocal };
      const work = await repository.createWork({ assetId: plan.asset_id, assetVersion: asset.version, title: plan.title, priority: plan.priority, ownerId: plan.owner_id, scheduledAt: plan.next_due_at, reason: input.reason }, `${key}:work`);
      generation = { workOrderId: work.id, assetVersion: asset.version, next, definition };
      result = { ...result, workOrderId: work.id, nextDueAt: next.at, nextDueLocal: next.local };
    } else {
      ports.authorizeReview();
      if (plan.status === "cancelled" || input.action === "pause" && plan.status !== "active") return fail(409, "MAINTENANCE_PLAN_STATE", "Reload an active plan before changing its schedule.");
      status = input.action === "pause" ? "paused" : "cancelled";
    }
    return repository.change(command, plan, { now, status, result: { ...result, status }, generation });
  }
  async function preventivePlanWork(): Promise<WorkItem[]> {
    if (!ports.canRead()) return [];
    return (await preventivePlans()).map(plan => {
      const terminal = plan.status === "cancelled", review = ports.canReview(), own = plan.owner_id === ports.actorId && Boolean(Number(plan.owner_eligible)), due = plan.next_due_at <= ports.clock.now() + plan.lead_days * 86400000;
      const canAct = ports.canWrite() && !terminal && (plan.status === "draft" || plan.status === "paused" ? review && plan.edited_by !== ports.actorId : plan.status === "active" && due && (own || review));
      return { source: { kind: "maintenance_plan", id: plan.id, version: plan.version, href: `/maintenance/plans/${plan.id}`, label: `Preventive maintenance · ${plan.title}` }, id: plan.id, type: "maintenance_plan", title: `${plan.asset_code} · ${plan.title}`, detail: `Preventive · every ${plan.interval_days} days · ${plan.time_zone}`, href: `/maintenance/plans/${plan.id}`, status: plan.status, updatedAt: plan.updated_at, dueAt: plan.next_due_at, queue: terminal ? "completed" : canAct ? "todo" : "waiting", canAct, responsibleQueue: "Preventive maintenance owner", nextAction: plan.status === "active" ? "Generate due work" : plan.status === "cancelled" ? "No further action" : "Review and activate preventive plan", blocker: Number(plan.owner_eligible) ? null : "Assigned maintenance owner is no longer eligible; pause and reassign the plan", relationships: [plan.owner_id === ports.actorId ? "assigned" : plan.created_by === ports.actorId ? "requested" : "team"] };
    });
  }
  return { preventivePlans, preventivePlanDetail, savePreventivePlan, changePreventivePlan, preventivePlanWork };
}
