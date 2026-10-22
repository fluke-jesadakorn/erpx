import {serviceMetricHistoryAccess,serviceRecordSourcePredicate} from "@erpx/server-field_service/adapters/record-purchases";
import {ApiError} from "@erpx/server-runtime/public/http";
import { finalRootOwnerProjectionSources as organizationProjectionSources } from "@erpx/server-organization/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as people_payrollProjectionSources } from "@erpx/server-people_payroll/public/final-root-owner-projection-sources";
import { readMetricSources } from '@erpx/server-runtime/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@erpx/feature-shared/lib/executive/canvas";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { specs } from '@erpx/server-runtime/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@erpx/server-runtime/public/executive/metric-projection-contract';
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const access=await serviceMetricHistoryAccess(sourceDb,a);if(access[id as keyof typeof access]===false)throw new ApiError(403,"METRIC_SOURCE_UNAVAILABLE","This Service metric requires purchased history and its current source authority.");
    const source=await serviceRecordSourcePredicate(sourceDb,a,"o");
    const spec = specs[id], scopedAlias = (alias: string) => `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`, scopeArgs = [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId];
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (id === "open-service-orders") {
        const label = breakdown === "status" ? "o.status"
            : breakdown === "branch" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=o.branch_id AND n.company_id=o.company_id),'Branch')`
                : breakdown === "department" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=o.department_id AND n.company_id=o.company_id),'Unassigned')`
                    : "o.status";
        sql = `SELECT ${label} label,count(*)::text value FROM field_service_orders o WHERE ${scopedAlias("o")} AND ${source.where} AND o.status NOT IN ('completed','cancelled','billed') GROUP BY 1 ORDER BY 1`;
        values = [...scopeArgs,...source.args];
    }
    if (id === "service-billing-releases") {
        const label = breakdown === "branch" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=o.branch_id AND n.company_id=o.company_id),'Branch')`
            : breakdown === "department" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=o.department_id AND n.company_id=o.company_id),'Unassigned')`
                : "to_char(to_timestamp(b.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')";
        sql = `SELECT ${label} label,count(*)::text value FROM field_service_billing_releases b JOIN field_service_orders o ON o.id=b.order_id WHERE ${scopedAlias("o")} AND ${source.where} AND b.created_at>=? AND b.created_at<? GROUP BY 1 ORDER BY 1`;
        values = [...scopeArgs,...source.args, Date.parse(f.from), Date.parse(f.to) + 86400000];
    }
    if (id === "field-visit-completion") {
        const label = breakdown === "month" ? "to_char(to_timestamp(o.scheduled_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
            : breakdown === "status" ? "o.status"
                : breakdown === "branch" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=o.branch_id AND n.company_id=o.company_id),'Branch')`
                    : breakdown === "department" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=o.department_id AND n.company_id=o.company_id),'Unassigned')`
                        : breakdown === "technician" ? `COALESCE((SELECT p.legal_name FROM (${people_payrollProjectionSources.hr_people}) p WHERE p.company_id=o.company_id AND p.user_id=o.technician_id AND p.merged_into_id IS NULL LIMIT 1),'Unassigned')`
                            : "'Field visits'";
        sql = `SELECT ${label} label,count(*) FILTER (WHERE o.status IN ('completed','billed') AND o.visit_completed_at IS NOT NULL)::text numerator,count(*)::text denominator
      FROM field_service_orders o WHERE ${scopedAlias("o")} AND ${source.where} AND o.scheduled_at IS NOT NULL
        AND o.status IN ('scheduled','dispatched','in_visit','exception','completed','billed')
        AND o.scheduled_at>=? AND o.scheduled_at<? GROUP BY 1 ORDER BY 1`;
        values = [...scopeArgs,...source.args, Date.parse(f.from), Date.parse(f.to) + 86400000];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: async (group, reported) => {const current=await serviceMetricHistoryAccess(sourceDb,a);if(current[id as keyof typeof current]===false)throw new ApiError(403,"METRIC_SOURCE_UNAVAILABLE","The current Service source authority changed.");return readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported);}, watermarks: async () => [] };
}
