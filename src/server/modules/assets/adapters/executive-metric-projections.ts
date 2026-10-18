import {assetMetricSource} from "./metric-purchases";
import { finalRootOwnerProjectionSources as organizationProjectionSources } from "../../organization/public/final-root-owner-projection-sources";
import { readMetricSources } from '@/server/modules/runtime/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@/lib/shared/executive/canvas";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from '@/server/modules/runtime/public/runtime/types';
import { specs } from '@/server/modules/runtime/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@/server/modules/runtime/public/executive/metric-projection-contract';
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id], scopedAlias = (alias: string) => `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`, scopeArgs = [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId];
    const source=await assetMetricSource(sourceDb,a,id);
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (id === "active-assets" || id === "asset-condition-location") {
        const label = breakdown === "branch" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=a.branch_id AND n.company_id=a.company_id),'Company-wide')`
            : breakdown === "department" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=a.department_id AND n.company_id=a.company_id),'Unassigned')`
                : "a.condition";
        sql = `SELECT ${label} label,count(*)::text value FROM (${source.sql}) a WHERE ${scopedAlias("a")} AND a.lifecycle_status='in_service' GROUP BY 1 ORDER BY 1`;
        values = scopeArgs;
    }
    if (id === "asset-retirements") {
        const label = breakdown === "branch"
            ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=a.branch_id AND n.company_id=a.company_id),'Company-wide')`
            : breakdown === "month" || breakdown === "default"
                ? "to_char(to_timestamp(e.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
                : "'Retired assets'";
        sql = `SELECT ${label} label,count(*)::text value FROM operational_asset_events e
      JOIN (${source.sql}) a ON a.id=e.asset_id
      WHERE ${scopedAlias("a")} AND e.action IN ('retire','retire-approve') AND e.lifecycle_status='retired' AND e.created_at>=? AND e.created_at<?
      GROUP BY 1 ORDER BY 1`;
        values = [...scopeArgs, Date.parse(f.from), Date.parse(f.to) + 86400000];
    }
    values=[...source.args,...values];
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: async (group, reported) => {await assetMetricSource(sourceDb,a,id);return readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported);}, watermarks: async () => [] };
}
