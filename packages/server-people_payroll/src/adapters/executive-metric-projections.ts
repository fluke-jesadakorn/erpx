import {peoplePayrollFinalOwnerProjections as organizationFinalOwnerProjections} from "@erpx/server-organization/public/people-payroll-final-owner-projections";
const organizationFinalSources=organizationFinalOwnerProjections();
import {peoplePayrollFinalOwnerProjections as financeFinalOwnerProjections} from "@erpx/server-finance/public/people-payroll-final-owner-projections";
const financeFinalSources=financeFinalOwnerProjections();
import { readMetricSources } from '@erpx/server-runtime/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@erpx/feature-shared/lib/executive/canvas";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { participantSql } from "@erpx/server-workflow/public/approvals";
import { specs } from '@erpx/server-runtime/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@erpx/server-runtime/public/executive/metric-projection-contract';
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id], scopedAlias = (alias: string) => `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`, scopeArgs = [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId];
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (id === "headcount") {
        const label = breakdown === "branch" || breakdown === "department"
            ? `COALESCE((SELECT display_name ${organizationFinalSources.executive_metric_projections_organization_nodes_from} WHERE id=e.${breakdown}_id AND company_id=e.company_id),'Unassigned')`
            : breakdown === "level" ? `COALESCE((SELECT 'Level '||level_number||' · '||name ${organizationFinalSources.executive_metric_projections_staff_levels_from} WHERE id=e.level_id AND company_id=e.company_id),'Unmapped')` : "'Active people'";
        sql = `WITH effective AS (SELECT DISTINCT ON (p.id) p.id effective_person_id,e.* FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE ${scopedAlias("e")} AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) ORDER BY p.id,e.start_date DESC,e.id) SELECT ${label} label,count(*)::text value FROM effective e GROUP BY 1 ORDER BY 1`;
        values = scopeArgs;
    }
    if (id === "approved-leave-duration") {
        const label = breakdown === "branch"
            ? `COALESCE((SELECT display_name ${organizationFinalSources.executive_metric_projections_organization_nodes_from} WHERE id=l.branch_id AND company_id=l.company_id),'Unassigned branch')`
            : breakdown === "department"
                ? `COALESCE((SELECT display_name ${organizationFinalSources.executive_metric_projections_organization_nodes_from} WHERE id=l.department_id AND company_id=l.company_id),'Unassigned department')`
                : "substring(l.start_date,1,7)";
        const readable = `(l.created_by=? OR l.decided_by=? OR ${participantSql("leave", "l", "?", "?")} OR ${participantSql("leave_cancellation", "l", "?", "?")})`;
        sql = `SELECT ${label} label,'days' unit,COALESCE(sum(COALESCE(l.calculated_units::numeric,l.days::numeric)*1000)::bigint,0)::text value
      FROM leave_requests l WHERE l.company_id=? AND l.state='approved' AND l.start_date BETWEEN ? AND ?
        AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.department_id=?) AND ${readable}
      GROUP BY 1 ORDER BY 1`;
        values = [a.companyId, f.from, f.to, a.branchId, a.branchId, a.departmentId, a.departmentId, a.userId, a.userId, a.userId, a.assignmentId, a.userId, a.assignmentId];
    }
    if (id === "authorized-overtime-duration") {
        const label = breakdown === "branch"
            ? `COALESCE((SELECT display_name ${organizationFinalSources.executive_metric_projections_organization_nodes_from} WHERE id=o.branch_id AND company_id=o.company_id),'Unassigned branch')`
            : breakdown === "department"
                ? `COALESCE((SELECT display_name ${organizationFinalSources.executive_metric_projections_organization_nodes_from} WHERE id=o.department_id AND company_id=o.company_id),'Unassigned department')`
                : "substring(h.work_date,1,7)";
        const readable = `(o.created_by=? OR ${participantSql("overtime", "o", "?", "?")})`;
        sql = `SELECT ${label} label,sum(h.minutes::numeric*60000)::bigint::text value
      FROM overtime_payroll_handoffs h JOIN overtime_requests o ON o.id=h.request_id AND o.company_id=h.company_id
      WHERE o.company_id=? AND o.state='approved' AND h.work_date BETWEEN ? AND ?
        AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?) AND ${readable}
      GROUP BY 1 ORDER BY 1`;
        values = [a.companyId, f.from, f.to, a.branchId, a.branchId, a.departmentId, a.departmentId, a.userId, a.userId, a.assignmentId];
    }
    if (id === "payroll-summary") {
        const label = breakdown === "month" ? "substring(r.period_end,1,7)" : "'Posted net payroll'";
        sql = `SELECT ${label} label,COALESCE(sum(i.net_minor::numeric),0)::text value
      FROM hr_payroll_runs r
      JOIN hr_payroll_items i ON i.run_id=r.id AND i.status='posted'
      WHERE r.company_id=? AND r.currency=? AND r.period_end BETWEEN ? AND ? AND r.status='posted'
        AND EXISTS(SELECT 1 FROM hr_payroll_postings p ${financeFinalSources.executive_metric_projections_finance_journals_j_join} ON j.id=p.journal_id
          WHERE p.run_id=r.id AND j.company_id=r.company_id AND j.status='posted')
      GROUP BY 1 ORDER BY 1`;
        values = [a.companyId, f.currency, f.from, f.to];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: (group, reported) => readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
