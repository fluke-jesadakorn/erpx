import { readMetricSources } from '@erpx/server-runtime/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@erpx/feature-shared/lib/executive/canvas";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { specs } from '@erpx/server-runtime/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@erpx/server-runtime/public/executive/metric-projection-contract';
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id];
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (id === "active-organization-nodes") {
        const label = breakdown === "status" ? "n.kind"
            : breakdown === "branch" ? `COALESCE((SELECT b.display_name FROM organization_nodes b WHERE b.id=CASE WHEN n.kind='branch' THEN n.id ELSE n.branch_id END AND b.company_id=n.company_id),'Branch')`
                : "n.kind";
        sql = `SELECT ${label} label,count(*)::text value FROM organization_nodes n WHERE n.company_id=? AND n.kind IN ('branch','department') AND n.status='active' AND (?::text IS NULL OR n.id=? OR n.branch_id=?) AND (?::text IS NULL OR n.id=?) GROUP BY 1 ORDER BY 1`;
        values = [a.companyId, a.branchId, a.branchId, a.branchId, a.departmentId, a.departmentId];
    }
    if (id === "pending-structure-changes") {
        const label = breakdown === "status" ? "c.node_kind"
            : breakdown === "branch" ? `COALESCE((SELECT display_name FROM organization_nodes WHERE id=c.branch_id AND company_id=c.company_id),'Company-wide')`
                : "c.node_kind";
        sql = `SELECT ${label} label,count(*)::text value FROM organization_node_changes c WHERE c.company_id=? AND c.state='submitted' AND (?::text IS NULL OR c.branch_id=?) GROUP BY 1 ORDER BY 1`;
        values = [a.companyId, a.branchId, a.branchId];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: (group, reported) => readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
