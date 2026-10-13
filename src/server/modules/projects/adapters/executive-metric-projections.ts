import { inventoryExecutiveOwnerProjections as financeProjectionSources } from "../../finance/public/inventory-executive-owner-projections";
import { inventoryExecutiveOwnerProjections as people_payrollProjectionSources } from "../../people_payroll/public/inventory-executive-owner-projections";
import { readMetricSources } from '@/server/modules/runtime/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@/lib/shared/executive/canvas";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from '@/server/modules/runtime/public/runtime/types';
import { projectDependencyBlockerCohort, projectMilestoneOverdueCohort } from '@/server/modules/runtime/public/executive/project-milestone-access';
import { specs } from '@/server/modules/runtime/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@/server/modules/runtime/public/executive/metric-projection-contract';
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id], scopedAlias = (alias: string) => `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`, scopeArgs = [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId];
    let sql = '', values: unknown[] = [], referenceAtOverride: number | null = null;
    if (id === "projects") {
        sql = `SELECT to_char(to_timestamp(p.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') label,count(*)::text value FROM staff_projects p WHERE ${scopedAlias("p")} AND (p.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?)) AND p.created_at>=? AND p.created_at<? GROUP BY 1 ORDER BY 1`;
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
      JOIN staff_projects p ON p.id=m.project_id AND p.company_id=m.company_id
      LEFT JOIN (${people_payrollProjectionSources.hr_people}) h ON h.company_id=m.company_id AND h.user_id=m.owner_user_id AND h.merged_into_id IS NULL
      WHERE ${cohort.sql} ${grouped} ORDER BY 1 LIMIT 101`;
        values = cohort.values;
    }
    if (id === "project-dependency-blockers") {
        const label = breakdown === "project" ? "p.title" : "'Blocked project dependencies'";
        const cohort = projectDependencyBlockerCohort(a, f);
        const grouping = breakdown === "project" ? "GROUP BY p.id" : "";
        sql = `SELECT min(${label}) label,count(*)::text value,count(*)::text count_value
      FROM staff_project_milestones m JOIN staff_projects p ON p.id=m.project_id AND p.company_id=m.company_id
      WHERE ${cohort.sql} ${grouping} ORDER BY 1 LIMIT 101`;
        values = cohort.values;
    }
    if (id === "project-actual-hours") {
        const label = breakdown === "month" ? "substring(l.work_date::text,1,7)"
            : breakdown === "project" ? "p.title" : "'Tracked project hours'";
        const group = breakdown === "month" ? "substring(l.work_date::text,1,7)"
            : breakdown === "project" ? "p.id" : null;
        const grouping = group ? `GROUP BY ${group}` : "";
        sql = `SELECT min(${label}) label,(sum(l.minutes::numeric)*60000)::text value,count(*)::text count_value
      FROM staff_project_work_logs l JOIN staff_projects p ON p.id=l.project_id AND p.company_id=l.company_id
      WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?)
        AND (p.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?))
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
        sql = `SELECT min(${label}) label,sum(a.amount_minor::numeric)::text value,count(*)::text count_value
      FROM staff_project_cost_allocations a JOIN staff_projects p ON p.id=a.project_id AND p.company_id=a.company_id
      JOIN (${financeProjectionSources.finance_journal_lines}) l ON l.id=a.journal_line_id JOIN (${financeProjectionSources.finance_journals}) j ON j.id=l.journal_id
      JOIN (${financeProjectionSources.finance_accounts}) ac ON ac.id=l.account_id
      WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?)
        AND (p.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?))
        AND a.state='approved' AND j.status='posted' AND ac.category IN ('expense','other_expense')
        AND j.currency=? AND j.entry_date BETWEEN ? AND ?
      ${grouping} ORDER BY 1 LIMIT 101`;
        values = [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId, a.userId, a.userId, f.currency, f.from, f.to];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: (group, reported) => readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
