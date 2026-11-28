import { readMetricSources } from '@erpx/platform-runtime-server/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@erpx/platform-shared-ui/lib/executive/canvas";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { specs } from '@erpx/platform-runtime-server/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@erpx/platform-runtime-server/public/executive/metric-projection-contract';
import { nativeExecutiveMetric } from './native-exec-metric/client';

/**
 * Ecommerce executive metric rows are read by the owner-local native axum
 * port. Rust owns the three count aggregates, the branch/department
 * organisation labels, the Company scope and the emitted statement and binds.
 * The retained source drill-down replays that trusted statement on the caller
 * unit; no TS SQL or fallback reader remains in this family.
 */
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id];
    const { raw, sql, values } = await nativeExecutiveMetric({
        companyId: a.companyId,
        branchId: a.branchId,
        departmentId: a.departmentId,
        metric: id,
        breakdown,
        from: f.from,
        to: f.to,
    }, a);
    const rows = raw as RawMetricRow[];
    return {
        raw: rows,
        referenceAtOverride: null,
        source: (group, reported) => readMetricSources(sourceDb, id, sql, [...values], group, rows, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported),
        watermarks: async () => [],
    };
}
