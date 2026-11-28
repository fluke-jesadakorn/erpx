import {marketingMetricSource} from "@erpx/marketing-server/adapters/metric-purchases";
import { readMetricSources } from '@erpx/platform-runtime-server/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@erpx/platform-shared-ui/lib/executive/canvas";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { specs } from '@erpx/platform-runtime-server/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@erpx/platform-runtime-server/public/executive/metric-projection-contract';
import {marketingMetricBreakdownSql} from './executive-metric-breakdown-sql';
import {marketingExecutiveLabelSql} from './executive-label-sql';
const branchLabel = marketingExecutiveLabelSql('branch');
const departmentLabel = marketingExecutiveLabelSql('department');
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id], scopedAlias = (alias: string) => `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`, scopeArgs = [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId];
    const purchased=await marketingMetricSource(sourceDb,a,id);
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (id === "marketing-lead-candidates") {
        const label = breakdown === "status" ? "c.status"
            : breakdown === "month" ? "to_char(to_timestamp(c.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
                : breakdown === "branch" ? branchLabel
                    : breakdown === "department" ? departmentLabel
                        : "c.status";
        sql = marketingMetricBreakdownSql('candidates', purchased.sql, label, scopedAlias('c'));
        values = [...purchased.args,...scopeArgs];
    }
    if (id === "marketing-engagement-events") {
        const label = breakdown === "status" ? "e.event_type"
            : breakdown === "branch" ? branchLabel
                : breakdown === "department" ? departmentLabel
                    : "to_char(to_timestamp(e.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')";
        sql = marketingMetricBreakdownSql('engagements', purchased.sql, label, scopedAlias('c'));
        values = [...purchased.args,...scopeArgs, Date.parse(f.from), Date.parse(f.to) + 86400000];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: async (group, reported) => {await marketingMetricSource(sourceDb,a,id);return readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported);}, watermarks: async () => [] };
}
