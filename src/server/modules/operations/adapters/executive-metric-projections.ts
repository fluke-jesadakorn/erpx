import { finalRootOwnerProjectionSources as organizationProjectionSources } from "../../organization/public/final-root-owner-projection-sources";
import { readMetricSources } from '@/server/modules/runtime/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@/lib/shared/executive/canvas";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from '@/server/modules/runtime/public/runtime/types';
import { operationsMetrics, operationsTaskCohort, operationsTaskVisibility, type OperationsMetricId } from '@/server/modules/runtime/public/executive/operations-access';
import { specs } from '@/server/modules/runtime/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@/server/modules/runtime/public/executive/metric-projection-contract';
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id], scopedAlias = (alias: string) => `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`, scopeArgs = [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId];
    let sql = '', values: unknown[] = [], referenceAtOverride: number | null = null;
    if (id === "my-tasks") {
        sql = `SELECT t.state label,count(*)::text value FROM staff_tasks t WHERE ${scopedAlias("t")} AND t.assignee_user_id=? AND t.state NOT IN ('draft','completed','cancelled') AND t.due_at>=? AND t.due_at<? GROUP BY 1 ORDER BY 1`;
        values = [...scopeArgs, a.userId, Date.parse(f.from), Date.parse(f.to) + 86400000];
    }
    if (operationsMetrics.includes(id as OperationsMetricId)) {
        const metric = id as OperationsMetricId, now = Date.now();
        referenceAtOverride = now;
        const visibility = operationsTaskVisibility(a, now), cohort = operationsTaskCohort(metric, "t", a, f, now);
        if (metric === "exception-resolution-time") {
            const label = breakdown === "month"
                ? `to_char(to_timestamp(t.updated_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')`
                : "'Median overdue time to completion'";
            sql = `WITH visible_tasks AS (
          SELECT t.* FROM staff_tasks t WHERE ${visibility.sql} AND ${cohort.sql}
        ), grouped AS (
          SELECT ${label} label,
            ROUND(percentile_cont(0.5) WITHIN GROUP (ORDER BY (t.updated_at-t.due_at)::double precision))::bigint::text value
          FROM visible_tasks t GROUP BY 1
        ), overall AS (
          SELECT count(*)::text count_value,
            COALESCE(ROUND(percentile_cont(0.5) WITHIN GROUP (ORDER BY (updated_at-due_at)::double precision))::bigint,0)::text total
          FROM visible_tasks
        )
        SELECT grouped.label,grouped.value,overall.total,overall.count_value FROM overall LEFT JOIN grouped ON TRUE ORDER BY grouped.label`;
            values = [...visibility.values, ...cohort.values];
        }
        else {
            const ageBand = `CASE WHEN t.due_at>=${now}-86400000 THEN 'Less than 1 day'
        WHEN t.due_at>=${now}-604800000 THEN '1–7 days'
        WHEN t.due_at>=${now}-2592000000 THEN '8–30 days' ELSE 'Over 30 days' END`;
            const label = breakdown === "status" ? "t.state"
                : breakdown === "branch" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=t.branch_id AND n.company_id=t.company_id),'Company-wide')`
                    : breakdown === "department" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=t.department_id AND n.company_id=t.company_id),'No department')`
                        : breakdown === "age-band" ? ageBand
                            : metric === "operations-escalations" ? "'Overdue escalations sent to current reader'"
                                : "'Overdue operational tasks'";
            sql = `SELECT ${label} label,count(*)::text value FROM staff_tasks t
        WHERE ${visibility.sql} AND ${cohort.sql} GROUP BY 1 ORDER BY 1`;
            values = [...visibility.values, ...cohort.values];
        }
    }
    if (id === "routines") {
        sql = `SELECT 'Published' label,count(*)::text value FROM routine_templates t WHERE ${scopedAlias("t")} AND t.assignee_user_id=? AND t.published=1`;
        values = [...scopeArgs, a.userId];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: (group, reported) => readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
