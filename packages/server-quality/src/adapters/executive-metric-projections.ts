import { commercialOwnerProjectionSources as organizationProjectionSources } from "@erpx/server-organization/public/commercial-owner-projection-sources";
import { readMetricSources } from '@erpx/server-runtime/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@erpx/feature-shared/lib/executive/canvas";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { specs } from '@erpx/server-runtime/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@erpx/server-runtime/public/executive/metric-projection-contract';
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id];
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (id === "quality-holds" || id === "quality-releases") {
        const releases = id === "quality-releases";
        const label = breakdown === "branch" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=h.branch_id AND n.company_id=h.company_id),'Company-wide')`
            : breakdown === "month" || releases && breakdown === "default" ? "to_char(to_timestamp(h.released_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
                : breakdown === "status" ? "h.status" : releases ? "'Released'" : "h.status";
        sql = `SELECT ${label} label,count(*)::text value FROM inventory_quality_holds h WHERE h.company_id=? AND (?::text IS NULL OR h.branch_id=?) AND ${releases ? "h.status='released' AND h.released_at>=? AND h.released_at<?" : "h.status NOT IN ('released','cancelled','disposed')"} GROUP BY 1 ORDER BY 1`;
        values = releases
            ? [a.companyId, a.branchId, a.branchId, Date.parse(f.from), Date.parse(f.to) + 86400000]
            : [a.companyId, a.branchId, a.branchId];
    }
    if (id === "inspection-pass-rate") {
        const label = breakdown === "month"
            ? "to_char(to_timestamp(i.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
            : breakdown === "branch"
                ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=h.branch_id AND n.company_id=h.company_id),'Company-wide')`
                : "'All finalized inspections'";
        sql = `SELECT ${label} label,
        count(*) FILTER (WHERE i.result='pass')::text numerator,count(*)::text denominator
      FROM inventory_quality_inspections i JOIN inventory_quality_holds h ON h.id=i.hold_id
      WHERE h.company_id=? AND (?::text IS NULL OR h.branch_id=?) AND i.created_at>=? AND i.created_at<?
      GROUP BY 1 ORDER BY 1`;
        values = [a.companyId, a.branchId, a.branchId, Date.parse(f.from), Date.parse(f.to) + 86400000];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: (group, reported) => readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
