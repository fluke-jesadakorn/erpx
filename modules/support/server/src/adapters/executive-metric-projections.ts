import { nativeSupportRuntime } from "@erpx/support-server/adapters/command-runtime";
import {
  nativeSupportReadPlan,
  SUPPORT_RD_EXEC_INCIDENT_METRIC,
  SUPPORT_RD_EXEC_TICKET_METRIC,
} from "@erpx/support-server/adapters/native-support-read";
import { supportTicketSourceIdSets } from "@erpx/support-server/adapters/ticket-source-read-policy";

const metricCapabilities = { "open-support-tickets": "support.tickets", "overdue-support-sla": "support.sla", "active-support-incidents": "support.incidents" } as const;
export async function supportMetricHistoryAccess(db: SqlDatabase, actor: WorkContext) {
    const runtime = nativeSupportRuntime(actor, db);
    const entries = await Promise.all(Object.entries(metricCapabilities).map(async ([id, capability]) => [id, (await runtime.purchase(capability)).history] as const));
    return Object.fromEntries(entries) as Record<keyof typeof metricCapabilities, boolean>;
}
import { readMetricSources } from '@erpx/platform-runtime-server/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@erpx/platform-shared-ui/lib/executive/canvas";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { specs } from '@erpx/platform-runtime-server/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@erpx/platform-runtime-server/public/executive/metric-projection-contract';

/**
 * Support executive metric rows are planned by the native Support `command-plan`
 * route. The authoritative reads stay on the Support-owned `support_tickets`
 * and `support_incidents`; the breakdown label, actor read predicate, Company
 * scope and (for tickets) the frozen POS/Assets/PMS source authorities are
 * fixed native SQL. Only scalar values cross the wire and the planned
 * statement runs on the caller's already-open unit. The same native-planned
 * SQL drives the bounded metric source drill-down.
 */
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    await nativeSupportRuntime(a, sourceDb).requirePurchase(metricCapabilities[id as keyof typeof metricCapabilities]);
    const spec = specs[id], referenceAtOverride: number | null = null, label = breakdown ?? '';
    const ticket = id === "open-support-tickets" || id === "overdue-support-sla";
    const overdue = id === "overdue-support-sla" ? 1 : 0;
    let plan: { sql: string; values: readonly unknown[]; results: RawMetricRow[] };
    if (ticket) {
        const source = await supportTicketSourceIdSets(a, sourceDb);
        plan = await nativeSupportReadPlan<RawMetricRow>(SUPPORT_RD_EXEC_TICKET_METRIC, [
            label, label, label, overdue,
            a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId,
            a.assignmentId, a.userId, a.role,
            JSON.stringify(source.posSale), JSON.stringify(source.asset), JSON.stringify(source.pmsRecord),
            overdue,
        ], sourceDb);
    } else {
        plan = await nativeSupportReadPlan<RawMetricRow>(SUPPORT_RD_EXEC_INCIDENT_METRIC, [
            label, label, label, label,
            a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId,
            a.assignmentId, a.userId, a.role,
        ], sourceDb);
    }
    const { sql, values, results: raw } = plan;
    return { raw, referenceAtOverride, source: (group, reported) => readMetricSources(sourceDb, id, sql, [...values], group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
