import {benefitMetricHistoryAccess,benefitMetricSourceScope,benefitMetricRecordSourceScope} from "@erpx/benefits-server/adapters/record-purchases";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import { readMetricSources } from '@erpx/platform-runtime-server/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@erpx/platform-shared-ui/lib/executive/canvas";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { specs } from '@erpx/platform-runtime-server/public/executive/metric-projection-contract';
import { benefitMetricCountSql } from '@erpx/benefits-server/adapters/benefits-read-sql';
import type { MetricSnapshot, RawMetricRow } from '@erpx/platform-runtime-server/public/executive/metric-projection-contract';
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, _breakdown: Breakdown): Promise<MetricSnapshot> {
    void _breakdown;
    const access=await benefitMetricHistoryAccess(sourceDb,a);if(access[id as keyof typeof access]===false)throw new ApiError(403,"METRIC_SOURCE_UNAVAILABLE","This Benefits metric requires its purchased history and current source authority.");
    const scope=await benefitMetricSourceScope(a);
    const spec = specs[id];
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (id === "active-benefit-elections") {
        sql = benefitMetricCountSql('e.status','benefit_elections','e',"status<>'cancelled'",' GROUP BY e.status ORDER BY e.status',scope.where);
        values = [a.companyId,...scope.args];
    }
    if (id === "published-benefit-plans") {
        sql = benefitMetricCountSql("'Published plans'",'benefit_plans','',"status='published'",'',scope.where);
        values = [a.companyId,...scope.args];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: async (group, reported) => {const current=await benefitMetricHistoryAccess(sourceDb,a);if(current[id as keyof typeof current]===false)throw new ApiError(403,"METRIC_SOURCE_UNAVAILABLE","The Benefits source authority changed.");const privateScope=id==="active-benefit-elections"?await benefitMetricRecordSourceScope(a):null;const sourceSql=privateScope?sql.replace(" GROUP BY e.status",` AND ${privateScope.where} GROUP BY e.status`):sql;return readMetricSources(sourceDb, id, sourceSql, privateScope?[...values,...privateScope.args]:values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported);}, watermarks: async () => [] };
}
