import type { Plan, PreventivePlanRow } from "@erpx/server-maintenance/domain/preventive-plan";
import type { WorkItem } from "@erpx/feature-work_projection/lib/work-item";
import type { Clock } from "@erpx/server-shared/application/ports";

export type PreventiveRepository = {
  plans(): Promise<PreventivePlanRow[]>; plan(id: string): Promise<Plan | null>;
  assetVersion(id: string): Promise<{ version: number } | null>;
  people(id: string): Promise<{ id: string; name: string }[]>; history(id: string): Promise<Record<string, unknown>[]>; occurrences(id: string): Promise<Record<string, unknown>[]>;
};
/** Retained preventive reads. Plan save/transition commands are native
 * (`erpx-server-maintenance`); this port only shapes the owner rows the
 * transport and Work projection still consume. */
export function createPreventiveApplication(ports: {
  actorId: string; authorize(write?: boolean): void; canRead(): boolean; canWrite(): boolean; canReview(): boolean;
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
    return { plan, actorId: ports.actorId, canWrite: ports.canWrite(), canReview: ports.canReview(), assetVersion: Number((await repository.assetVersion(plan.asset_id))?.version ?? null), people: await repository.people(id), history: await repository.history(id), occurrences: await repository.occurrences(id) };
  }
  async function preventivePlanWork(): Promise<WorkItem[]> {
    if (!ports.canRead()) return [];
    return (await preventivePlans()).map(plan => {
      const terminal = plan.status === "cancelled", review = ports.canReview(), own = plan.owner_id === ports.actorId && Boolean(Number(plan.owner_eligible)), due = plan.next_due_at <= ports.clock.now() + plan.lead_days * 86400000;
      const canAct = ports.canWrite() && !terminal && (plan.status === "draft" || plan.status === "paused" ? review && plan.edited_by !== ports.actorId : plan.status === "active" && due && (own || review));
      return { source: { kind: "maintenance_plan", id: plan.id, version: plan.version, href: `/maintenance/plans/${plan.id}`, label: `Preventive maintenance · ${plan.title}` }, id: plan.id, type: "maintenance_plan", title: `${plan.asset_code} · ${plan.title}`, detail: `Preventive · every ${plan.interval_days} days · ${plan.time_zone}`, href: `/maintenance/plans/${plan.id}`, status: plan.status, updatedAt: plan.updated_at, dueAt: plan.next_due_at, queue: terminal ? "completed" : canAct ? "todo" : "waiting", canAct, responsibleQueue: "Preventive maintenance owner", nextAction: plan.status === "active" ? "Generate due work" : plan.status === "cancelled" ? "No further action" : "Review and activate preventive plan", blocker: Number(plan.owner_eligible) ? null : "Assigned maintenance owner is no longer eligible; pause and reassign the plan", relationships: [plan.owner_id === ports.actorId ? "assigned" : plan.created_by === ports.actorId ? "requested" : "team"] };
    });
  }
  return { preventivePlans, preventivePlanDetail, preventivePlanWork };
}
