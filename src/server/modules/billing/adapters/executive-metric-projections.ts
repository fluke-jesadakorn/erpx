import { readMetricSources } from '@/server/modules/runtime/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@/lib/shared/executive/canvas";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from '@/server/modules/runtime/public/runtime/types';
import { specs } from '@/server/modules/runtime/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@/server/modules/runtime/public/executive/metric-projection-contract';
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id];
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (id === "active-subscriptions") {
        const label = breakdown === "status" ? "status" : breakdown === "plan" ? "plan" : "'Active subscriptions'", now = Date.now();
        sql = `WITH subscriptions AS (
        SELECT e.id,COALESCE(p.name,e.plan_id) plan,CASE WHEN e.pending_end THEN 'ending' ELSE 'active' END status
        FROM billing_enrollments e
        LEFT JOIN LATERAL (SELECT name FROM billing_plans WHERE id=e.plan_id ORDER BY revision DESC LIMIT 1) p ON TRUE
        WHERE e.holding_id=? AND e.started_at<=? AND e.ended_at IS NULL
          AND (e.pending_end=FALSE OR e.effective_at IS NULL OR e.effective_at>?)
        UNION ALL
        SELECT b.holding_id id,'Legacy subscription' plan,b.status
        FROM beta_billing b WHERE b.holding_id=? AND b.subscription_id IS NOT NULL AND b.status IN ('active','trialing')
          AND (b.current_period_end IS NULL OR b.current_period_end*1000>?)
      ) SELECT ${label} label,count(*)::text value FROM subscriptions GROUP BY 1 ORDER BY 1`;
        values = [a.holdingId, now, now, a.holdingId, now];
    }
    if (id === "overdue-invoices") {
        const label = breakdown === "month" ? "to_char(to_timestamp(i.due_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
            : breakdown === "status" ? "i.status" : "'Overdue subscription invoices'";
        sql = `SELECT ${label} label,count(*)::text value FROM billing_invoices i
      WHERE i.holding_id=? AND i.status IN ('open','uncollectible') AND i.due_at>=? AND i.due_at<? AND i.due_at<?
      GROUP BY 1 ORDER BY 1`;
        values = [a.holdingId, Date.parse(f.from), Date.parse(f.to) + 86400000, Date.now()];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: (group, reported) => readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
