import {crmMetricSource} from "./metric-purchases";
import { commercialOwnerProjectionSources as commerceProjectionSources } from "../../commerce/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as organizationProjectionSources } from "../../organization/public/commercial-owner-projection-sources";
import { readMetricSources } from '@/server/modules/runtime/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@/lib/shared/executive/canvas";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from '@/server/modules/runtime/public/runtime/types';
import { specs } from '@/server/modules/runtime/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@/server/modules/runtime/public/executive/metric-projection-contract';
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id], scopedAlias = (alias: string) => `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`, scopeArgs = [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId];
    const purchased=await crmMetricSource(sourceDb,a,id);
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (id === "opportunity-pipeline") {
        const label = breakdown === "status" ? "v.stage"
            : breakdown === "branch" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=l.branch_id AND n.company_id=l.company_id),'Company-wide')`
                : breakdown === "month" ? "substring(v.close_date,1,7)" : "v.stage";
        sql = `SELECT ${label} label,sum(v.expected_minor)::text value FROM crm_opportunities v JOIN (${purchased.sql}) l ON l.id=v.lead_id WHERE ${scopedAlias("l")} AND v.currency=? AND v.close_date BETWEEN ? AND ? AND v.stage IN ('discovery','proposal','negotiation') GROUP BY 1 ORDER BY 1`;
        values = [...purchased.args,...scopeArgs, f.currency, f.from, f.to];
    }
    if (id === "overdue-followups") {
        const label = breakdown === "status" ? "c.status"
            : breakdown === "branch" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=o.branch_id AND n.company_id=o.company_id),'Company-wide')`
                : breakdown === "department" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=o.department_id AND n.company_id=o.company_id),'Unassigned')`
                    : breakdown === "month" ? "substring(c.due_date,1,7)" : "'Overdue follow-ups'";
        sql = `SELECT ${label} label,count(*)::text value FROM (${purchased.sql}) c JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=c.order_id AND o.company_id=c.company_id WHERE ${scopedAlias("o")} AND o.kind IN ('sales','quote') AND c.status='open' AND c.due_date BETWEEN ? AND ? AND c.due_date<to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') GROUP BY 1 ORDER BY 1`;
        values = [...purchased.args,...scopeArgs, f.from, f.to];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: async (group, reported) => {await crmMetricSource(sourceDb,a,id);return readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported);}, watermarks: async () => [] };
}
