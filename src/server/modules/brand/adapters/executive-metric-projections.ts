import { commercialOwnerProjectionSources as organizationProjectionSources } from "../../organization/public/commercial-owner-projection-sources";
import { readMetricSources } from '@/server/modules/runtime/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@/lib/executive/canvas";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from '@/server/modules/runtime/public/runtime/types';
import { specs } from '@/server/modules/runtime/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@/server/modules/runtime/public/executive/metric-projection-contract';
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id];
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (id === "brand-profiles" || id === "brand-profile-updates") {
        const updates = id === "brand-profile-updates";
        const label = updates && (breakdown === "month" || breakdown === "default")
            ? "to_char(to_timestamp(p.updated_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
            : breakdown === "branch"
                ? "b.display_name"
                : updates ? "'Updated profiles'" : "'Configured profiles'";
        sql = `SELECT ${label} label,count(*)::text value FROM organization_brand_profiles p
      JOIN (${organizationProjectionSources.organization_nodes}) b ON b.id=p.scope_id AND b.company_id=p.company_id AND b.kind='branch' AND b.status='active'
      WHERE p.scope_kind='branch' AND p.company_id=? AND (?::text IS NULL OR p.scope_id=?) ${updates ? "AND p.updated_at>=? AND p.updated_at<?" : ""} GROUP BY 1 ORDER BY 1`;
        values = [a.companyId, a.branchId, a.branchId, ...(updates ? [Date.parse(f.from), Date.parse(f.to) + 86400000] : [])];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: (group, reported) => readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
