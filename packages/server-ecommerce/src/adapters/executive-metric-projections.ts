import { readMetricSources } from '@erpx/server-runtime/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@erpx/feature-shared/lib/executive/canvas";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { specs } from '@erpx/server-runtime/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@erpx/server-runtime/public/executive/metric-projection-contract';
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
