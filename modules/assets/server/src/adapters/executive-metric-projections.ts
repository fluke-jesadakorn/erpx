import {assetMetricSource} from "@erpx/assets-server/adapters/metric-purchases";
import { finalRootOwnerProjectionSources as organizationProjectionSources } from "@erpx/organization-server/public/final-root-owner-projection-sources";
import { readMetricSources } from '@erpx/platform-runtime-server/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@erpx/platform-shared-ui/lib/executive/canvas";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { specs } from '@erpx/platform-runtime-server/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@erpx/platform-runtime-server/public/executive/metric-projection-contract';
import {assetProjectionFromSql} from "./asset-scope-tuple";
/** Owner-local fixed label projections shared by the Assets executive breakdowns.
 * Branch and department labels generate one parameterized org-node lookup literal
 * so the TS read tail carries no duplicate statement. */
export const assetNodeDisplayNameLabel = (column: "branch_id" | "department_id", fallback: string) =>
    `COALESCE((${assetProjectionFromSql("n.display_name",` (${organizationProjectionSources.organization_nodes}) n`,` WHERE n.id=a.${column} AND n.company_id=a.company_id`)}),'${fallback}')`;
/** Canonical owner-local fixed aggregate prefix for Assets executive breakdowns.
 * The in-service and retirement projections share this one literal so the TS
 * read tail carries no duplicate statement. Callers pass only an owner-owned
 * label expression; no SQL or relation name is accepted from a request. */
export const assetMetricBreakdownProjectionSql = (label: string) =>
    assetProjectionFromSql(`${label} label,count(*)::text value`,"");
export const assetBranchDisplayNameLabel = assetNodeDisplayNameLabel("branch_id", "Company-wide");
const assetDepartmentDisplayNameLabel = assetNodeDisplayNameLabel("department_id", "Unassigned");
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id], scopedAlias = (alias: string) => `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`, scopeArgs = [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId];
    const source=await assetMetricSource(sourceDb,a,id);
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (id === "active-assets" || id === "asset-condition-location") {
        const label = breakdown === "branch" ? assetBranchDisplayNameLabel
            : breakdown === "department" ? assetDepartmentDisplayNameLabel
                : "a.condition";
        sql = `${assetMetricBreakdownProjectionSql(label)} (${source.sql}) a WHERE ${scopedAlias("a")} AND a.lifecycle_status='in_service' GROUP BY 1 ORDER BY 1`;
        values = scopeArgs;
    }
    if (id === "asset-retirements") {
        const label = breakdown === "branch"
            ? assetBranchDisplayNameLabel
            : breakdown === "month" || breakdown === "default"
                ? "to_char(to_timestamp(e.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
                : "'Retired assets'";
        sql = `${assetMetricBreakdownProjectionSql(label)} operational_asset_events e
      JOIN (${source.sql}) a ON a.id=e.asset_id
      WHERE ${scopedAlias("a")} AND e.action IN ('retire','retire-approve') AND e.lifecycle_status='retired' AND e.created_at>=? AND e.created_at<?
      GROUP BY 1 ORDER BY 1`;
        values = [...scopeArgs, Date.parse(f.from), Date.parse(f.to) + 86400000];
    }
    values=[...source.args,...values];
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: async (group, reported) => {await assetMetricSource(sourceDb,a,id);return readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported);}, watermarks: async () => [] };
}
