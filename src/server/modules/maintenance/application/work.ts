import type { Row } from "../../commerce/public/commerce";
import type { Clock } from "../../shared/application/ports";
import { createWorkSchema, workActionSchema, maintenanceState, maintenanceStates, type Work, type MaintenanceWork, type MaintenanceState, type MaintenanceCreateInput, type MaintenanceAction, type MaintenanceDowntimeEvidenceRow } from "../domain/work";
import type { WorkItem } from "@/lib/work-item";

export type MaintenanceCommand<Input = MaintenanceAction> = { key: string; operation: string; id: string; input: Input };
export type DowntimeQuery = { from: number; to: number; query: string; limit: number; cursor?: { endedAt: number; id: string }; recordId?: string };
export type MaintenanceRepository = {
  replay(command: MaintenanceCommand<MaintenanceAction | MaintenanceCreateInput>): Promise<Record<string, unknown> | null>;
  items(): Promise<Work[]>; assets(): Promise<Row[]>; people(): Promise<Row[]>;
  work(id: string): Promise<MaintenanceWork | null>;
  spares(branchId: string): Promise<Awaited<ReturnType<typeof import("../../inventory/public/inventory-consumption").availableServiceSpares>>>;
  consumptions(id: string): Promise<Row[]>; history(id: string): Promise<Row[]>;
  downtime(input: DowntimeQuery): Promise<MaintenanceDowntimeEvidenceRow[]>;
  create(command: MaintenanceCommand<MaintenanceCreateInput>, status: string, now: number): Promise<Record<string, unknown>>;
  prepareConsumption(command: MaintenanceCommand, source: MaintenanceWork): Promise<void>;
  change(command: MaintenanceCommand, source: MaintenanceWork, state: MaintenanceState, now: number): Promise<Record<string, unknown>>;
};
export function createMaintenanceApplication(ports: {
  actorId: string; authorize(write?: boolean): void; canRead(): boolean; canWrite(): boolean; canReview(): boolean;
  clock: Clock; repository(): Promise<MaintenanceRepository>; error(status: number, code: string, message: string): Error;
}) {
  const fail = (status: number, code: string, message: string): never => { throw ports.error(status, code, message); };
  async function maintenanceWorkspace() {
    ports.authorize(); const repository = await ports.repository();
    return { items: await repository.items(), assets: await repository.assets(), people: await repository.people(), canCreate: ports.canWrite() };
  }
  async function maintenanceDetail(id: string) {
    const data = await maintenanceWorkspace(), work = data.items.find(item => item.id === id);
    if (!work) return fail(404, "MAINTENANCE_WORK_NOT_FOUND", "Maintenance work order not found in this scope.");
    const repository = await ports.repository();
    return { ...data, work, spares: await repository.spares(String(work.branch_id)), consumptions: await repository.consumptions(id), history: await repository.history(id) };
  }
  async function maintenanceDowntimeEvidenceRows(input: DowntimeQuery) { ports.authorize(); return (await ports.repository()).downtime(input); }
  async function createMaintenanceWork(raw: unknown, key: string) {
    ports.authorize(true); const input = createWorkSchema.parse(raw), repository = await ports.repository(), command = { key, operation: "maintenance.create", id: `maintenance-${key}`, input }, prior = await repository.replay(command);
    if (prior) return prior;
    const now = ports.clock.now(), status = input.scheduledAt ? "scheduled" : "open";
    return repository.create(command, status, now);
  }
  async function changeMaintenanceWork(id: string, raw: unknown, key: string) {
    ports.authorize(true); const input = workActionSchema.parse(raw), repository = await ports.repository(), command = { key, operation: `maintenance.${input.action}`, id, input }, prior = await repository.replay(command);
    if (prior) return prior;
    const source = await repository.work(id);
    if (!source) return fail(404, "MAINTENANCE_WORK_NOT_FOUND", "Maintenance work order not found in this scope.");
    const manage = ports.canReview() && Number(source.can_review) > 0, own = source.owner_id === ports.actorId && Number(source.owner_eligible) > 0;
    if (!own && !manage) return fail(403, "MAINTENANCE_OWNER_REQUIRED", "The current owner or an eligible reviewer must perform this action.");
    if (!maintenanceStates[input.action].includes(source.status)) return fail(409, "MAINTENANCE_STATE_CHANGED", "Refresh the work order before continuing.");
    const now = ports.clock.now(), state = maintenanceState(source, input, now);
    if (input.action === "issue-part") await repository.prepareConsumption(command, source);
    return repository.change(command, source, state, now);
  }
  async function maintenanceWork(): Promise<WorkItem[]> {
    if (!ports.canRead()) return [];
    const { items } = await maintenanceWorkspace();
    return items.map(work => {
      const terminal = ["completed", "cancelled"].includes(work.status), manage = ports.canReview() && Number(work.can_review) > 0, own = work.owner_id === ports.actorId && Number(work.owner_eligible) > 0, canAct = !terminal && (own || manage);
      return { id: String(work.id), type: "maintenance_work", title: `${work.asset_code} · ${work.title}`, detail: `${work.asset_name} · ${work.priority} priority`, href: `/maintenance/${work.id}`, status: String(work.status), updatedAt: Number(work.updated_at), dueAt: work.scheduled_at ? Number(work.scheduled_at) : null, queue: terminal ? "completed" : canAct ? "todo" : "waiting", canAct, responsibleQueue: "Assigned maintenance owner", nextAction: work.status === "blocked" ? "Resolve blocker and resume" : work.status === "in_progress" ? "Record completion evidence" : "Schedule or start work", blocker: Number(work.owner_eligible) ? work.blocked_reason : "Assigned maintenance owner is no longer eligible; reviewer reassignment is required", relationships: [work.owner_id === ports.actorId ? "assigned" : work.created_by === ports.actorId ? "requested" : manage ? "team" : "team"] };
    });
  }
  return { maintenanceWorkspace, maintenanceDetail, maintenanceDowntimeEvidenceRows, createMaintenanceWork, changeMaintenanceWork, maintenanceWork };
}
