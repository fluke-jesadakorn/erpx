import {benefitMetricHistoryAccess,benefitMetricSourceScope,benefitMetricRecordSourceScope} from "@erpx/server-benefits/adapters/record-purchases";
import {ApiError} from "@erpx/server-runtime/public/http";
import { readMetricSources } from '@erpx/server-runtime/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@erpx/feature-shared/lib/executive/canvas";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { specs } from '@erpx/server-runtime/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@erpx/server-runtime/public/executive/metric-projection-contract';
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, _breakdown: Breakdown): Promise<MetricSnapshot> {
    void _breakdown;
    const access=await benefitMetricHistoryAccess(sourceDb,a);if(access[id as keyof typeof access]===false)throw new ApiError(403,"METRIC_SOURCE_UNAVAILABLE","This Benefits metric requires its purchased history and current source authority.");
    const scope=await benefitMetricSourceScope(a);
    const spec = specs[id];
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (id === "active-benefit-elections") {
        sql = `SELECT e.status label,count(*)::text value FROM benefit_elections e WHERE e.company_id=? AND ${scope.where} AND e.status<>'cancelled' AND e.effective_from<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.effective_to IS NULL OR e.effective_to>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) GROUP BY e.status ORDER BY e.status`;
        values = [a.companyId,...scope.args];
    }
    if (id === "published-benefit-plans") {
        sql = `SELECT 'Published plans' label,count(*)::text value FROM benefit_plans WHERE company_id=? AND ${scope.where} AND status='published' AND effective_from<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (effective_to IS NULL OR effective_to>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))`;
        values = [a.companyId,...scope.args];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: async (group, reported) => {const current=await benefitMetricHistoryAccess(sourceDb,a);if(current[id as keyof typeof current]===false)throw new ApiError(403,"METRIC_SOURCE_UNAVAILABLE","The Benefits source authority changed.");const privateScope=id==="active-benefit-elections"?await benefitMetricRecordSourceScope(a):null;const sourceSql=privateScope?sql.replace(" GROUP BY e.status",` AND ${privateScope.where} GROUP BY e.status`):sql;return readMetricSources(sourceDb, id, sourceSql, privateScope?[...values,...privateScope.args]:values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported);}, watermarks: async () => [] };
}
