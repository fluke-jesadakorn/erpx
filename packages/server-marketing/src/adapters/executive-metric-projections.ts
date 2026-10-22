import {marketingMetricSource} from "@erpx/server-marketing/adapters/metric-purchases";
import { commercialOwnerProjectionSources as organizationProjectionSources } from "@erpx/server-organization/public/commercial-owner-projection-sources";
import { readMetricSources } from '@erpx/server-runtime/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@erpx/feature-shared/lib/executive/canvas";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { specs } from '@erpx/server-runtime/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@erpx/server-runtime/public/executive/metric-projection-contract';
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id], scopedAlias = (alias: string) => `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`, scopeArgs = [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId];
    const purchased=await marketingMetricSource(sourceDb,a,id);
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (id === "marketing-lead-candidates") {
        const label = breakdown === "status" ? "c.status"
            : breakdown === "month" ? "to_char(to_timestamp(c.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
                : breakdown === "branch" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=c.branch_id AND n.company_id=c.company_id),'Branch')`
                    : breakdown === "department" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=c.department_id AND n.company_id=c.company_id),'Unassigned')`
                        : "c.status";
        sql = `SELECT ${label} label,count(*)::text value FROM (${purchased.sql}) c WHERE ${scopedAlias("c")} GROUP BY 1 ORDER BY 1`;
        values = [...purchased.args,...scopeArgs];
    }
    if (id === "marketing-engagement-events") {
        const label = breakdown === "status" ? "e.event_type"
            : breakdown === "branch" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=c.branch_id AND n.company_id=c.company_id),'Branch')`
                : breakdown === "department" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=c.department_id AND n.company_id=c.company_id),'Unassigned')`
                    : "to_char(to_timestamp(e.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')";
        sql = `SELECT ${label} label,count(*)::text value FROM marketing_engagement_events e JOIN (${purchased.sql}) c ON c.id=e.candidate_id WHERE ${scopedAlias("c")} AND e.created_at>=? AND e.created_at<? GROUP BY 1 ORDER BY 1`;
        values = [...purchased.args,...scopeArgs, Date.parse(f.from), Date.parse(f.to) + 86400000];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: async (group, reported) => {await marketingMetricSource(sourceDb,a,id);return readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported);}, watermarks: async () => [] };
}
