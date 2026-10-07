import { inventoryExecutiveOwnerProjections as organizationProjectionSources } from "../../organization/public/inventory-executive-owner-projections";
import { readMetricSources } from '@/server/modules/runtime/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@/lib/executive/canvas";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from '@/server/modules/runtime/public/runtime/types';
import { specs } from '@/server/modules/runtime/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@/server/modules/runtime/public/executive/metric-projection-contract';
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id];
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (id === "planning-shortage-scenarios") {
        const label = breakdown === "status" ? "s.status"
            : breakdown === "branch" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=s.branch_id AND n.company_id=s.company_id),'Branch')`
                : "substring(s.due_date,1,7)";
        sql = `SELECT ${label} label,count(*)::text value FROM planning_scenarios s WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.due_date BETWEEN ? AND ? AND jsonb_path_exists(COALESCE(s.shortages_json::jsonb,'[]'::jsonb),'$[*] ? (@.shortageMilli > 0)') GROUP BY 1 ORDER BY 1`;
        values = [a.companyId, a.branchId, a.branchId, f.from, f.to];
    }
    if (["proposed-planning-recommendations", "capacity-feasible-recommendations"].includes(id)) {
        const feasible = id === "capacity-feasible-recommendations";
        const label = breakdown === "status" ? "r.status"
            : breakdown === "branch" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=s.branch_id AND n.company_id=s.company_id),'Branch')`
                : "substring(r.feasible_date,1,7)";
        sql = `SELECT ${label} label,count(*)::text value FROM planning_recommendations r JOIN planning_scenarios s ON s.id=r.scenario_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND ${feasible ? "r.status IN ('proposed','approved') AND r.required_minutes<=s.capacity_minutes" : "r.status='proposed'"} AND r.feasible_date BETWEEN ? AND ? GROUP BY 1 ORDER BY 1`;
        values = [a.companyId, a.branchId, a.branchId, f.from, f.to];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: (group, reported) => readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
