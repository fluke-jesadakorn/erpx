import { readMetricSources } from '@erpx/server-runtime/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@erpx/feature-shared/lib/executive/canvas";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { specs } from '@erpx/server-runtime/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@erpx/server-runtime/public/executive/metric-projection-contract';
import { BILLING_PLAN_NAME_LATERAL_SQL } from './billing-read-tail-r18-sql';
import { billingLegacySubscriptionSql } from './billing-read-tail-r32-sql';
import { billingMetricCountSql } from './billing-read-tail-r27-sql';
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id];
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (id === "active-subscriptions") {
        const label = breakdown === "status" ? "status" : breakdown === "plan" ? "plan" : "'Active subscriptions'", now = Date.now();
        sql = `WITH subscriptions AS (
        SELECT e.id,COALESCE(p.name,e.plan_id) plan,CASE WHEN e.pending_end THEN 'ending' ELSE 'active' END status
        FROM billing_enrollments e
        ${BILLING_PLAN_NAME_LATERAL_SQL}
        WHERE e.holding_id=? AND e.started_at<=? AND e.ended_at IS NULL
          AND (e.pending_end=FALSE OR e.effective_at IS NULL OR e.effective_at>?)
        UNION ALL
${billingLegacySubscriptionSql("b.holding_id id,'Legacy subscription' plan,b.status", "        ")}
      ) ${billingMetricCountSql(label)}subscriptions GROUP BY 1 ORDER BY 1`;
        values = [a.holdingId, now, now, a.holdingId, now];
    }
    if (id === "overdue-invoices") {
        const label = breakdown === "month" ? "to_char(to_timestamp(i.due_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
            : breakdown === "status" ? "i.status" : "'Overdue subscription invoices'";
        sql = `${billingMetricCountSql(label)}billing_invoices i
      WHERE i.holding_id=? AND i.status IN ('open','uncollectible') AND i.due_at>=? AND i.due_at<? AND i.due_at<?
      GROUP BY 1 ORDER BY 1`;
        values = [a.holdingId, Date.parse(f.from), Date.parse(f.to) + 86400000, Date.now()];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: (group, reported) => readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
