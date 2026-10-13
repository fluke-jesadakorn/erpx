import { hospitalityOwnerProjectionSources as organizationProjectionSources } from "../../organization/public/hospitality-owner-projection-sources";
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
    if (id === "pos-sales") {
        sql = `SELECT ${breakdown === "status" ? "s.state" : breakdown === "branch" ? `COALESCE((SELECT display_name FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE id=s.branch_id AND company_id=s.company_id),'Company-wide')` : "substring(s.business_date,1,7)"} label,sum(s.total_minor)::text value FROM pos_sales s WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=? AND s.business_date BETWEEN ? AND ? AND s.state='completed' GROUP BY 1 ORDER BY 1`;
        values = [a.companyId, a.branchId, a.branchId, f.currency, f.from, f.to];
    }
    if (id === "net-pos-sales") {
        const label = breakdown === "branch"
            ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=x.branch_id AND n.company_id=x.company_id),'Company-wide')`
            : "substring(x.event_date,1,7)";
        sql = `WITH sales AS (
        SELECT s.id,s.id sale_id,s.company_id,s.branch_id,s.business_date event_date,s.total_minor amount_minor
        FROM pos_sales s WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=?
          AND s.business_date BETWEEN ? AND ? AND s.state='completed'
      ), refunds AS (
        SELECT r.id,s.id sale_id,s.company_id,s.branch_id,r.accounting_date event_date,-r.amount_minor amount_minor
        FROM pos_refunds r JOIN pos_sales s ON s.id=r.sale_id AND s.company_id=r.company_id
        WHERE r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND s.currency=?
          AND r.accounting_date BETWEEN ? AND ? AND r.kind='sale' AND r.state='completed'
      ), net_events AS (SELECT * FROM sales UNION ALL SELECT * FROM refunds)
      SELECT ${label} label,COALESCE(sum(x.amount_minor),0)::text value FROM net_events x GROUP BY 1 ORDER BY 1`;
        values = [a.companyId, a.branchId, a.branchId, f.currency, f.from, f.to, a.companyId, a.branchId, a.branchId, f.currency, f.from, f.to];
    }
    if (id === "pos-payment-mix") {
        const label = breakdown === "branch"
            ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=s.branch_id AND n.company_id=s.company_id),'Company-wide')`
            : breakdown === "month" ? "substring(s.business_date,1,7)" : "p.method";
        sql = `SELECT ${label} label,COALESCE(sum(p.amount_minor-COALESCE((SELECT sum(rp.amount_minor) FROM pos_refund_payments rp
        JOIN pos_refunds r ON r.id=rp.refund_id AND r.company_id=p.company_id
        WHERE rp.payment_id=p.id AND rp.state='succeeded' AND r.kind='sale'),0)),0)::text value
      FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id AND s.company_id=p.company_id
      WHERE p.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=? AND s.business_date BETWEEN ? AND ?
        AND s.state='completed' AND p.state='succeeded' GROUP BY 1 ORDER BY 1`;
        values = [a.companyId, a.branchId, a.branchId, f.currency, f.from, f.to];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: (group, reported) => readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
