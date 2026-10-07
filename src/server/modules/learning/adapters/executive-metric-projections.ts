import {learningTalentSnapshotSources as organizationSnapshotSources} from '../../organization/public/learning-talent-snapshot-sources';
const organizationSources=organizationSnapshotSources();
import { readMetricSources } from '@/server/modules/runtime/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@/lib/executive/canvas";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from '@/server/modules/runtime/public/runtime/types';
import { specs } from '@/server/modules/runtime/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@/server/modules/runtime/public/executive/metric-projection-contract';
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id], scopedAlias = (alias: string) => `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`, scopeArgs = [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId];
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (id === "learning-completions") {
        const label = breakdown === "branch" ? `COALESCE((SELECT n.display_name ${organizationSources.learning_adapters_executive_metric_projections_organization_nodes_n_from} WHERE n.id=a.branch_id AND n.company_id=a.company_id),'Branch')`
            : breakdown === "department" ? `COALESCE((SELECT n.display_name ${organizationSources.learning_adapters_executive_metric_projections_organization_nodes_n_from} WHERE n.id=a.department_id AND n.company_id=a.company_id),'Unassigned')`
                : "to_char(to_timestamp(e.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')";
        sql = `SELECT ${label} label,count(*)::text value FROM learning_assignment_events e JOIN learning_assignments a ON a.id=e.assignment_id JOIN learning_courses c ON c.id=a.course_id WHERE ${scopedAlias("a")} AND e.action='complete' AND e.status='completed' AND e.created_at>=? AND e.created_at<? GROUP BY 1 ORDER BY 1`;
        values = [...scopeArgs, Date.parse(f.from), Date.parse(f.to) + 86400000];
    }
    if (id === "overdue-required-learning") {
        const label = breakdown === "status" ? "a.status"
            : breakdown === "branch" ? `COALESCE((SELECT n.display_name ${organizationSources.learning_adapters_executive_metric_projections_organization_nodes_n_from} WHERE n.id=a.branch_id AND n.company_id=a.company_id),'Branch')`
                : breakdown === "department" ? `COALESCE((SELECT n.display_name ${organizationSources.learning_adapters_executive_metric_projections_organization_nodes_n_from} WHERE n.id=a.department_id AND n.company_id=a.company_id),'Unassigned')`
                    : "c.title";
        sql = `SELECT ${label} label,count(*)::text value FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id WHERE ${scopedAlias("a")} AND c.required_for IS NOT NULL AND a.due_date<to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND a.status IN ('assigned','in_progress') GROUP BY 1 ORDER BY 1`;
        values = scopeArgs;
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: (group, reported) => readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
