import {projectMetricSource} from "@erpx/server-projects/adapters/metric-purchases";
import {projectMemberScopeSql} from "@erpx/server-projects/adapters/project-read-projections";
import {projectActualHoursFrom,projectMilestoneOwnerJoin,projectMilestoneProjectJoin,projectPostedCostFrom} from "./executive-evidence-chains";
import { readMetricSources } from '@erpx/server-runtime/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@erpx/feature-shared/lib/executive/canvas";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { projectDependencyBlockerCohort, projectMilestoneOverdueCohort } from '@erpx/server-runtime/public/executive/project-milestone-access';
import { specs } from '@erpx/server-runtime/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@erpx/server-runtime/public/executive/metric-projection-contract';
/** Byte-identical grouped metric projection shared by the dependency-blocker,
 * actual-hours and posted-cost executive metric reads. Only the closed label and
 * value expressions vary; the value/count aliases and bind order stay exact. */
const groupedMetricSelect = (label: string, value: string) => `SELECT min(${label}) label,${value} value,count(*)::text count_value`;
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id], scopedAlias = (alias: string) => `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`, scopeArgs = [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId];
    const projectSource=await projectMetricSource(sourceDb,a,id);
    let sql = '', values: unknown[] = [], referenceAtOverride: number | null = null;
    if (id === "projects") {
        sql = `SELECT to_char(to_timestamp(p.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') label,count(*)::text value FROM (${projectSource.sql}) p WHERE ${scopedAlias("p")} AND ${projectMemberScopeSql} AND p.created_at>=? AND p.created_at<? GROUP BY 1 ORDER BY 1`;
        values = [...scopeArgs, a.userId, a.userId, Date.parse(f.from), Date.parse(f.to) + 86400000];
    }
    if (id === "overdue-project-milestones") {
        referenceAtOverride = Date.now();
        const ageBand = `CASE WHEN ${referenceAtOverride}-m.due_at<604800000 THEN '0-7 days' WHEN ${referenceAtOverride}-m.due_at<2592000000 THEN '8-30 days' ELSE 'Over 30 days' END`;
        const label = breakdown === "project" ? "p.title"
            : breakdown === "owner" ? "COALESCE(h.legal_name,m.owner_user_id)"
                : breakdown === "age-band" ? ageBand : "'Overdue project milestones'";
        const group = breakdown === "project" ? "p.id"
            : breakdown === "owner" ? "m.owner_user_id"
                : breakdown === "age-band" ? ageBand : "1";
        const cohort = projectMilestoneOverdueCohort(a, f, referenceAtOverride);
        const grouped = breakdown === "default" ? "" : `GROUP BY ${group}`;
        sql = `SELECT min(${label}) label,count(*)::text value FROM staff_project_milestones m
      ${projectMilestoneProjectJoin(projectSource.sql)}
      ${projectMilestoneOwnerJoin()}
      WHERE ${cohort.sql} ${grouped} ORDER BY 1 LIMIT 101`;
        values = cohort.values;
    }
    if (id === "project-dependency-blockers") {
        const label = breakdown === "project" ? "p.title" : "'Blocked project dependencies'";
        const cohort = projectDependencyBlockerCohort(a, f);
        const grouping = breakdown === "project" ? "GROUP BY p.id" : "";
        sql = `${groupedMetricSelect(label,'count(*)::text')}
      FROM staff_project_milestones m JOIN (${projectSource.sql}) p ON p.id=m.project_id AND p.company_id=m.company_id
      WHERE ${cohort.sql} ${grouping} ORDER BY 1 LIMIT 101`;
        values = cohort.values;
    }
    if (id === "project-actual-hours") {
        const label = breakdown === "month" ? "substring(l.work_date::text,1,7)"
            : breakdown === "project" ? "p.title" : "'Tracked project hours'";
        const group = breakdown === "month" ? "substring(l.work_date::text,1,7)"
            : breakdown === "project" ? "p.id" : null;
        const grouping = group ? `GROUP BY ${group}` : "";
        sql = `${groupedMetricSelect(label,'(sum(l.minutes::numeric)*60000)::text')}
      ${projectActualHoursFrom(projectSource.sql)}
      WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?)
        AND ${projectMemberScopeSql}
        AND l.state='approved' AND l.work_date BETWEEN ? AND ?
      ${grouping} ORDER BY 1 LIMIT 101`;
        values = [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId, a.userId, a.userId, f.from, f.to];
    }
    if (id === "project-posted-cost") {
        const label = breakdown === "month" ? "substring(j.entry_date::text,1,7)"
            : breakdown === "project" ? "p.title" : "'Posted project cost'";
        const group = breakdown === "month" ? "substring(j.entry_date::text,1,7)"
            : breakdown === "project" ? "p.id" : null;
        const grouping = group ? `GROUP BY ${group}` : "";
        sql = `${groupedMetricSelect(label,'sum(a.amount_minor::numeric)::text')}
      ${projectPostedCostFrom(projectSource.sql)}
      WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?)
        AND ${projectMemberScopeSql}
        AND a.state='approved' AND j.status='posted' AND ac.category IN ('expense','other_expense')
        AND j.currency=? AND j.entry_date BETWEEN ? AND ?
      ${grouping} ORDER BY 1 LIMIT 101`;
        values = [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId, a.userId, a.userId, f.currency, f.from, f.to];
    }
    values=[...projectSource.args,...values];
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: async (group, reported) => {await projectMetricSource(sourceDb,a,id);return readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported);}, watermarks: async () => [] };
}
