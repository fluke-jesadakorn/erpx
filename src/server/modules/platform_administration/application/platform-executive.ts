import type { Clock } from "../../shared/application/ports";
import { dateFilters, executiveSourceSchema, metricTotal, type Group, type PlatformExecutiveSnapshot } from "../domain/platform-executive";
export type PlatformExecutiveRepository = {
  failed(fromAt: number, toAt: number): Promise<Group[]>; abnormal(): Promise<Group[]>;
  failedSources(fromAt: number, toAt: number, label: string, offset: number): Promise<{ created_at: number; action: string }[]>;
  abnormalSources(label: string, offset: number): Promise<{ component: string; status: string; checked_at: number }[]>;
};
export function requirePlatformExecutiveAccess(allowed: boolean, error: (status: number, code: string, message: string) => Error) {
  if (!allowed) throw error(403, "PLATFORM_EXECUTIVE_FORBIDDEN", "Your Platform role cannot read Executive insights.");
}
export function createPlatformExecutiveApplication(ports: { canRead(): boolean; clock: Clock; hash(value: string): Promise<string>; repository: PlatformExecutiveRepository; error(status: number, code: string, message: string): Error }) {
  function requirePlatformExecutive() { requirePlatformExecutiveAccess(ports.canRead(), ports.error); }
  async function platformExecutiveSnapshot(rawFilters?: unknown): Promise<PlatformExecutiveSnapshot> {
    requirePlatformExecutive();
    const year = new Date(ports.clock.now()).getUTCFullYear(), filters = dateFilters.parse(rawFilters ?? { from: `${year}-01-01`, to: new Date(ports.clock.now()).toISOString().slice(0, 10) }), fromAt = Date.parse(filters.from), toAt = Date.parse(filters.to) + 86_400_000;
    const [failed, abnormal] = await Promise.all([ports.repository.failed(fromAt, toAt), ports.repository.abnormal()]);
    const sourceRevision = await ports.hash(JSON.stringify({ provisioningFailures: failed, abnormalServices: abnormal })), referencedAt = ports.clock.now();
    const metrics: PlatformExecutiveSnapshot["metrics"] = [
      { id: "failed-provisioning", label: "Failed provisioning jobs", formula: "Count immutable platform_holding_provisioning_events with action failed in the selected UTC interval.", dateBasis: "Failure event time (UTC)", total: metricTotal(failed, ports.error), rows: failed, status: failed.length ? "ready" : "empty", sourceReferences: [{ dataset: "platform_holding_provisioning_events", revision: sourceRevision, referencedAt }] },
      { id: "abnormal-platform-services", label: "Abnormal platform services", formula: "Count monitored platform_health components whose current status is outside healthy, ready, and ok.", dateBasis: "Latest health observation", total: metricTotal(abnormal, ports.error), rows: abnormal, status: abnormal.length ? "ready" : "empty", sourceReferences: [{ dataset: "platform_health", revision: sourceRevision, referencedAt }] },
    ];
    return { scope: "platform", referenceAt: referencedAt, filters, sourceRevision, metrics };
  }
  async function platformExecutiveSources(raw: unknown) {
    requirePlatformExecutive();
    const input = executiveSourceSchema.parse(raw), filters = dateFilters.parse({ from: input.from, to: input.to });
    const rows = input.metric === "failed-provisioning" ? await ports.repository.failedSources(Date.parse(filters.from), Date.parse(filters.to) + 86400000, input.label, (input.page - 1) * 30) : await ports.repository.abnormalSources(input.label, (input.page - 1) * 30);
    return { id: input.label, name: input.label, metric: input.metric, filters, rows: rows.slice(0, 30), page: input.page, hasNext: rows.length > 30 };
  }
  return { requirePlatformExecutive, platformExecutiveSnapshot, platformExecutiveSources };
}
