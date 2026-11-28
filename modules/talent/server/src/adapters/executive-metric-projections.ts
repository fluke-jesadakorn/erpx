import {talentMetricSource} from "@erpx/talent-server/adapters/metric-purchases";
import {selectProjection} from './talent-read-tail-r37-sources';
import {learningTalentSnapshotSources as organizationSnapshotSources} from '@erpx/organization-server/public/learning-talent-snapshot-sources';
const organizationSources=organizationSnapshotSources();
import { readMetricSources } from '@erpx/platform-runtime-server/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@erpx/platform-shared-ui/lib/executive/canvas";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { specs } from '@erpx/platform-runtime-server/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@erpx/platform-runtime-server/public/executive/metric-projection-contract';
const orgNameLabel = (ref: string, fallback: string) => `COALESCE((${selectProjection(`n.display_name ${organizationSources.talent_adapters_executive_metric_projections_organization_nodes_n_from} WHERE n.id=${ref} AND n.company_id=r.company_id`)}),'${fallback}')`;
const metricSelect = (label: string, sourceSql: string) => `${selectProjection(label)} label,count(*)::text value FROM (${sourceSql}) `;
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id], scopedAlias = (alias: string) => `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`, scopeArgs = [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId];
    const source=await talentMetricSource(sourceDb,a,id);
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (id === "open-requisitions") {
        const label = breakdown === "status" ? "r.status"
            : breakdown === "branch" ? orgNameLabel("r.branch_id", "Branch")
                : breakdown === "department" ? orgNameLabel("r.department_id", "Unassigned")
                    : "r.job_title";
        sql = `${metricSelect(label, source.sql)}r WHERE ${scopedAlias("r")} AND r.status='open' GROUP BY 1 ORDER BY 1`;
        values = scopeArgs;
    }
    if (id === "application-stage-counts") {
        const label = breakdown === "branch" ? orgNameLabel("r.branch_id", "Branch")
            : breakdown === "department" ? orgNameLabel("r.department_id", "Unassigned")
                : "a.status";
        sql = `${metricSelect(label, source.sql)}a JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id WHERE ${scopedAlias("r")} AND a.updated_at>=? AND a.updated_at<? GROUP BY 1 ORDER BY 1`;
        values = [...scopeArgs, Date.parse(f.from), Date.parse(f.to) + 86400000];
    }
    values=[...source.args,...values];
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: async (group, reported) => {await talentMetricSource(sourceDb,a,id);return readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported);}, watermarks: async () => [] };
}
