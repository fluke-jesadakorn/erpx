import { finalRootOwnerProjectionSources as assetsProjectionSources } from "../../assets/public/final-root-owner-projection-sources";
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
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (id === "open-work-orders" || id === "blocked-maintenance") {
        const blocked = id === "blocked-maintenance";
        const label = breakdown === "branch" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=w.branch_id AND n.company_id=w.company_id),'Company-wide')`
            : breakdown === "department" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=w.department_id AND n.company_id=w.company_id),'Unassigned')`
                : blocked ? "'Blocked'" : "w.status";
        sql = `SELECT ${label} label,count(*)::text value FROM maintenance_work_orders w WHERE ${scopedAlias("w")} AND ${blocked ? "w.status='blocked'" : "w.status NOT IN ('completed','cancelled')"} GROUP BY 1 ORDER BY 1`;
        values = scopeArgs;
    }
    if (id === "asset-downtime") {
        const label = breakdown === "asset" ? "s.asset_code || ' · ' || s.asset_name"
            : breakdown === "branch" ? "COALESCE(s.branch_name,'Company-wide')"
                : breakdown === "department" ? "COALESCE(s.department_name,'Unassigned')" : "s.month_label";
        const from = Date.parse(`${f.from}T00:00:00.000Z`), to = Date.parse(f.to) + 86400000;
        sql = `WITH raw_intervals AS (
        SELECT w.asset_id,w.branch_id,w.department_id,a.code asset_code,a.name asset_name,b.display_name branch_name,d.display_name department_name,
          greatest(w.downtime_started_at,?::bigint) start_at,least(w.downtime_ended_at,?::bigint) end_at,w.id work_order_id
        FROM maintenance_work_orders w JOIN (${assetsProjectionSources.operational_assets}) a ON a.id=w.asset_id AND a.company_id=w.company_id
        LEFT JOIN (${organizationProjectionSources.organization_nodes}) b ON b.id=w.branch_id AND b.company_id=w.company_id
        LEFT JOIN (${organizationProjectionSources.organization_nodes}) d ON d.id=w.department_id AND d.company_id=w.company_id
        WHERE ${scopedAlias("w")} AND w.status IN ('completed','cancelled')
          AND w.downtime_started_at IS NOT NULL AND w.downtime_ended_at IS NOT NULL
          AND w.downtime_ended_at>w.downtime_started_at AND w.downtime_started_at<? AND w.downtime_ended_at>?
      ), ordered AS (
        SELECT *,max(end_at) OVER(PARTITION BY asset_id ORDER BY start_at,end_at,work_order_id ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING) prior_end
        FROM raw_intervals
      ), marked AS (
        SELECT *,sum(CASE WHEN prior_end IS NULL OR start_at>prior_end THEN 1 ELSE 0 END)
          OVER(PARTITION BY asset_id ORDER BY start_at,end_at,work_order_id ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) interval_group
        FROM ordered
      ), merged AS (
        SELECT asset_id,min(asset_code) asset_code,min(asset_name) asset_name,min(branch_name) branch_name,min(department_name) department_name,
          interval_group,min(start_at) start_at,max(end_at) end_at
        FROM marked GROUP BY asset_id,interval_group
      ), monthly_segments AS (
        SELECT m.*,to_char(month_start,'YYYY-MM') month_label,
          greatest(m.start_at,floor(extract(epoch FROM month_start AT TIME ZONE 'UTC')*1000)::bigint) segment_start,
          least(m.end_at,floor(extract(epoch FROM (month_start+interval '1 month') AT TIME ZONE 'UTC')*1000)::bigint) segment_end
        FROM merged m CROSS JOIN LATERAL generate_series(
          date_trunc('month',to_timestamp(m.start_at/1000.0) AT TIME ZONE 'UTC'),
          date_trunc('month',to_timestamp((m.end_at-1)/1000.0) AT TIME ZONE 'UTC'),interval '1 month'
        ) AS months(month_start)
      )
      SELECT ${label} label,sum(segment_end-segment_start)::text value FROM monthly_segments s
      WHERE segment_end>segment_start GROUP BY 1 ORDER BY 1`;
        values = [from, to, ...scopeArgs, to, from];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: (group, reported) => readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
