import { finalRootOwnerProjectionSources as organizationProjectionSources } from "../../organization/public/final-root-owner-projection-sources";
import { readMetricSources } from '@/server/modules/runtime/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@/lib/shared/executive/canvas";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from '@/server/modules/runtime/public/runtime/types';
import { specs } from '@/server/modules/runtime/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@/server/modules/runtime/public/executive/metric-projection-contract';
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id], scopedAlias = (alias: string) => `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`, scopeArgs = [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId];
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (id === "open-support-tickets" || id === "overdue-support-sla") {
        const overdue = id === "overdue-support-sla";
        const label = breakdown === "status" ? "t.status"
            : breakdown === "branch" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=t.branch_id AND n.company_id=t.company_id),'Company-wide')`
                : breakdown === "department" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=t.department_id AND n.company_id=t.company_id),'Unassigned')`
                    : overdue ? "'Overdue SLA'" : "t.status";
        sql = `SELECT ${label} label,count(*)::text value FROM support_tickets t WHERE ${scopedAlias("t")} AND t.status NOT IN ('closed','cancelled') ${overdue ? "AND t.sla_clock_json->>'state'='running' AND (t.sla_clock_json->>'dueAt') ~ '^[0-9]+$' AND (t.sla_clock_json->>'dueAt')::bigint < floor(extract(epoch FROM clock_timestamp())*1000)::bigint" : ""} GROUP BY 1 ORDER BY 1`;
        values = scopeArgs;
    }
    if (id === "active-support-incidents") {
        const label = breakdown === "status" ? "t.status"
            : breakdown === "severity" ? "t.severity"
                : breakdown === "branch" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=t.branch_id AND n.company_id=t.company_id),'Company-wide')`
                    : breakdown === "department" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=t.department_id AND n.company_id=t.company_id),'Unassigned')`
                        : "'Active support incidents'";
        sql = `SELECT ${label} label,count(*)::text value FROM support_incidents t WHERE ${scopedAlias("t")} AND t.status IN ('open','investigating','mitigated') GROUP BY 1 ORDER BY 1`;
        values = scopeArgs;
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: (group, reported) => readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
