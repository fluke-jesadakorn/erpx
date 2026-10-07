import { commercialOwnerProjectionSources as organizationProjectionSources } from "../../organization/public/commercial-owner-projection-sources";
import { readMetricSources } from '@/server/modules/runtime/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@/lib/executive/canvas";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from '@/server/modules/runtime/public/runtime/types';
import { customerTimelineEvents } from "@/server/modules/customer_360/public/customer-timeline";
import { specs } from '@/server/modules/runtime/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@/server/modules/runtime/public/executive/metric-projection-contract';
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id], scopedAlias = (alias: string) => `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`, scopeArgs = [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId];
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (id === "sales-fulfillment-rate") {
        const label = breakdown === "month" ? "substring(o.order_date,1,7)"
            : breakdown === "status" ? "o.state"
                : breakdown === "branch" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=o.branch_id AND n.company_id=o.company_id),'Company-wide')`
                    : "'Eligible sales orders'";
        sql = `SELECT ${label} label,count(*) FILTER (WHERE o.state='fulfilled')::text numerator,count(*)::text denominator
      FROM commerce_orders o WHERE ${scopedAlias("o")} AND o.kind='sales'
        AND o.state IN ('confirmed','partially_fulfilled','fulfilled') AND o.order_date BETWEEN ? AND ?
        AND EXISTS(SELECT 1 FROM commerce_order_lines l WHERE l.order_id=o.id)
      GROUP BY 1 ORDER BY 1`;
        values = [...scopeArgs, f.from, f.to];
    }
    if (id === "sales-cancellations" || id === "sales-cancellation-value") {
        const label = breakdown === "branch"
            ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=o.branch_id AND n.company_id=o.company_id),'Company-wide')`
            : breakdown === "month"
                ? "to_char(to_timestamp(o.updated_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
                : id === "sales-cancellations" ? "'Cancelled sales orders'" : "'Confirmed cancellation value'";
        const amount = id === "sales-cancellations" ? "count(*)::text" : "sum(o.subtotal_minor::numeric+o.tax_minor::numeric)::text";
        const approved = id === "sales-cancellation-value" ? "AND o.approved_by IS NOT NULL AND o.currency=?" : "";
        sql = `SELECT ${label} label,${amount} value FROM commerce_orders o
      WHERE ${scopedAlias("o")} AND o.kind='sales' AND o.state='cancelled' ${approved}
        AND o.updated_at>=? AND o.updated_at<? GROUP BY 1 ORDER BY 1`;
        values = [...scopeArgs, ...(id === "sales-cancellation-value" ? [f.currency] : []), Date.parse(f.from), Date.parse(f.to) + 86400000];
    }
    if (id === "sales-orders" || id === "purchase-orders") {
        sql = `SELECT ${breakdown === "status" ? "o.state" : breakdown === "branch" ? `COALESCE((SELECT display_name FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE id=o.branch_id AND company_id=o.company_id),'Company-wide')` : "substring(o.order_date,1,7)"} label,sum(o.subtotal_minor+o.tax_minor)::text value FROM commerce_orders o WHERE ${scopedAlias("o")} AND o.kind=? AND o.currency=? AND o.order_date BETWEEN ? AND ? AND o.state IN ('confirmed','partially_fulfilled','fulfilled') GROUP BY 1 ORDER BY 1`;
        values = [...scopeArgs, id === "sales-orders" ? "sales" : "purchase", f.currency, f.from, f.to];
    }
    if (id === "open-po-receipts") {
        const label = breakdown === "status" ? "o.state"
            : breakdown === "branch" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=o.branch_id AND n.company_id=o.company_id),'Company-wide')`
                : breakdown === "month" ? "substring(o.due_date,1,7)" : "'Open PO receipts'";
        sql = `SELECT ${label} label,count(*)::text value FROM commerce_orders o
      WHERE ${scopedAlias("o")} AND o.kind='purchase' AND o.state IN ('confirmed','partially_fulfilled')
        AND o.due_date BETWEEN ? AND ?
        AND EXISTS(SELECT 1 FROM commerce_order_lines l WHERE l.order_id=o.id AND l.quantity_milli>l.fulfilled_milli)
      GROUP BY 1 ORDER BY 1`;
        values = [...scopeArgs, f.from, f.to];
    }
    if (id === "recent-customer-activity") {
        const source = customerTimelineEvents(a);
        const label = breakdown === "month"
            ? "to_char(to_timestamp(e.at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
            : "e.source_type";
        sql = `SELECT ${label} label,count(*)::text value FROM (${source.sql}) e WHERE e.at>=? AND e.at<? GROUP BY 1 ORDER BY 1`;
        values = [...source.values, Date.parse(f.from), Date.parse(f.to) + 86400000];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: (group, reported) => readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
