import { readMetricSources } from '@erpx/server-runtime/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@erpx/feature-shared/lib/executive/canvas";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { specs } from '@erpx/server-runtime/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@erpx/server-runtime/public/executive/metric-projection-contract';
import { CA_RD_DUPLICATE_REVIEWS_PENDING, nativeOwnerReadStatement } from './native-owner-read';
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id];
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (id === "duplicate-reviews-pending") {
        const statement = await nativeOwnerReadStatement(CA_RD_DUPLICATE_REVIEWS_PENDING, [a.companyId], a.holdingSlug);
        sql = statement.sql;
        values = [...statement.values];
    }
    if (id === "identity-change-events") {
        const label = breakdown === "month" || breakdown === "default"
            ? "to_char(to_timestamp(e.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
            : "e.action";
        sql = `SELECT ${label} label,count(*)::text value FROM customer_identity_change_events e JOIN customer_identity_changes c ON c.id=e.change_id WHERE c.company_id=? AND e.created_at>=? AND e.created_at<? GROUP BY 1 ORDER BY 1`;
        values = [a.companyId, Date.parse(f.from), Date.parse(f.to) + 86400000];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: (group, reported) => readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
