import {learningTalentSnapshotSources as organizationSnapshotSources} from '../../organization/public/learning-talent-snapshot-sources';
const organizationSources=organizationSnapshotSources();
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
    if (id === "open-requisitions") {
        const label = breakdown === "status" ? "r.status"
            : breakdown === "branch" ? `COALESCE((SELECT n.display_name ${organizationSources.talent_adapters_executive_metric_projections_organization_nodes_n_from} WHERE n.id=r.branch_id AND n.company_id=r.company_id),'Branch')`
                : breakdown === "department" ? `COALESCE((SELECT n.display_name ${organizationSources.talent_adapters_executive_metric_projections_organization_nodes_n_from} WHERE n.id=r.department_id AND n.company_id=r.company_id),'Unassigned')`
                    : "r.job_title";
        sql = `SELECT ${label} label,count(*)::text value FROM talent_requisitions r WHERE ${scopedAlias("r")} AND r.status='open' GROUP BY 1 ORDER BY 1`;
        values = scopeArgs;
    }
    if (id === "application-stage-counts") {
        const label = breakdown === "branch" ? `COALESCE((SELECT n.display_name ${organizationSources.talent_adapters_executive_metric_projections_organization_nodes_n_from} WHERE n.id=r.branch_id AND n.company_id=r.company_id),'Branch')`
            : breakdown === "department" ? `COALESCE((SELECT n.display_name ${organizationSources.talent_adapters_executive_metric_projections_organization_nodes_n_from} WHERE n.id=r.department_id AND n.company_id=r.company_id),'Unassigned')`
                : "a.status";
        sql = `SELECT ${label} label,count(*)::text value FROM talent_applications a JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id WHERE ${scopedAlias("r")} AND a.updated_at>=? AND a.updated_at<? GROUP BY 1 ORDER BY 1`;
        values = [...scopeArgs, Date.parse(f.from), Date.parse(f.to) + 86400000];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: (group, reported) => readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
