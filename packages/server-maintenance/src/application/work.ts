import type {Row} from '@erpx/server-commerce/public/commerce';
import type { Clock } from "@erpx/server-shared/application/ports";
import type { Work, MaintenanceDowntimeEvidenceRow } from "@erpx/server-maintenance/domain/work";
import type { WorkItem } from "@erpx/feature-work_projection/lib/work-item";

export type DowntimeQuery = { from: number; to: number; query: string; limit: number; cursor?: { endedAt: number; id: string }; recordId?: string };
export type MaintenanceRepository = {
  items(): Promise<Work[]>; assets(): Promise<Row[]>; people(): Promise<Row[]>; nodes?(): Promise<Row[]>;
  spares(branchId: string): Promise<Awaited<ReturnType<typeof import("@erpx/server-inventory/public/inventory-consumption").availableServiceSpares>>>;
  consumptions(id: string): Promise<Row[]>; history(id: string): Promise<Row[]>;
  downtime(input: DowntimeQuery): Promise<MaintenanceDowntimeEvidenceRow[]>;
};
/** Retained Maintenance reads. Work creation and transitions are native
 * (`erpx-server-maintenance`); this port only shapes the owner rows the
 * workspace, Work projection and downtime evidence still consume. */
export function createMaintenanceApplication(ports: {
  actorId: string; authorize(write?: boolean): void; canRead(): boolean; canWrite(): boolean; canReview(): boolean;
  clock: Clock; repository(): Promise<MaintenanceRepository>; error(status: number, code: string, message: string): Error;
}) {
  const fail = (status: number, code: string, message: string): never => { throw ports.error(status, code, message); };
  async function maintenanceWorkspace() {
    ports.authorize(); const repository = await ports.repository();
    return { items: await repository.items(), assets: await repository.assets(), people: await repository.people(), nodes: await repository.nodes?.()??[], canCreate: ports.canWrite() };
  }
  async function maintenanceDetail(id: string) {
    const data = await maintenanceWorkspace(), work = data.items.find(item => item.id === id);
    if (!work) return fail(404, "MAINTENANCE_WORK_NOT_FOUND", "Maintenance work order not found in this scope.");
    const repository = await ports.repository();
    return { ...data, work, spares: await repository.spares(String(work.branch_id)), consumptions: await repository.consumptions(id), history: await repository.history(id) };
  }
  async function maintenanceDowntimeEvidenceRows(input: DowntimeQuery) { ports.authorize(); return (await ports.repository()).downtime(input); }
  async function maintenanceWork(): Promise<WorkItem[]> {
    if (!ports.canRead()) return [];
    const { items } = await maintenanceWorkspace();
    return items.map(work => {
      const terminal = ["completed", "cancelled"].includes(work.status), manage = ports.canReview() && Number(work.can_review) > 0, own = work.owner_id === ports.actorId && Number(work.owner_eligible) > 0, canAct = !terminal && (own || manage);
      return { id: String(work.id), type: "maintenance_work", title: `${work.asset_code} · ${work.title}`, detail: `${work.asset_name} · ${work.priority} priority`, href: `/maintenance/${work.id}`, status: String(work.status), updatedAt: Number(work.updated_at), dueAt: work.scheduled_at ? Number(work.scheduled_at) : null, queue: terminal ? "completed" : canAct ? "todo" : "waiting", canAct, responsibleQueue: "Assigned maintenance owner", nextAction: work.status === "blocked" ? "Resolve blocker and resume" : work.status === "in_progress" ? "Record completion evidence" : "Schedule or start work", blocker: Number(work.owner_eligible) ? work.blocked_reason : "Assigned maintenance owner is no longer eligible; reviewer reassignment is required", relationships: [work.owner_id === ports.actorId ? "assigned" : work.created_by === ports.actorId ? "requested" : manage ? "team" : "team"] };
    });
  }
  return { maintenanceWorkspace, maintenanceDetail, maintenanceDowntimeEvidenceRows, maintenanceWork };
}
