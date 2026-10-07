import { inventoryExecutiveOwnerProjections as commerceProjectionSources } from "../../commerce/public/inventory-executive-owner-projections";
import { inventoryExecutiveOwnerProjections as financeProjectionSources } from "../../finance/public/inventory-executive-owner-projections";
import { inventoryExecutiveOwnerProjections as organizationProjectionSources } from "../../organization/public/inventory-executive-owner-projections";
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
    if (id === "completed-manufacturing-orders" || id === "manufacturing-orders-with-scrap") {
        const scrap = id === "manufacturing-orders-with-scrap";
        const label = breakdown === "branch" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=o.branch_id AND n.company_id=o.company_id),'Branch')`
            : "to_char(to_timestamp(o.updated_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')";
        sql = `SELECT ${label} label,count(*)::text value FROM manufacturing_orders o WHERE o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND o.status='completed' AND o.updated_at>=? AND o.updated_at<? ${scrap ? "AND o.reported_scrap_milli>0" : ""} GROUP BY 1 ORDER BY 1`;
        values = [a.companyId, a.branchId, a.branchId, Date.parse(f.from), Date.parse(f.to) + 86400000];
    }
    if (id === "production-cost") {
        const label = breakdown === "month" ? "to_char(to_timestamp(j.posted_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
            : breakdown === "branch" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=o.branch_id AND n.company_id=o.company_id),'Branch')`
                : breakdown === "product" ? "p.name"
                    : breakdown === "warehouse" ? "w.name"
                        : "'Posted production costs'";
        const groupKey = breakdown === "product" ? ",p.id" : breakdown === "warehouse" ? ",w.id" : breakdown === "branch" ? ",o.branch_id" : "";
        sql = `SELECT ${label} label,COALESCE(sum(m.amount_minor::numeric),0)::text value
      FROM manufacturing_cost_movements m
      JOIN manufacturing_orders o ON o.id=m.order_id AND o.company_id=m.company_id
      JOIN manufacturing_specifications s ON s.id=o.specification_id
      JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=s.output_product_id AND p.company_id=o.company_id
      JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=o.warehouse_id AND w.company_id=o.company_id
      JOIN (${financeProjectionSources.finance_journals}) j ON j.id=m.journal_id AND j.company_id=m.company_id AND j.status='posted' AND j.currency=?
      JOIN (${commerceProjectionSources.commerce_settings}) cs ON cs.company_id=o.company_id AND cs.currency=j.currency
      WHERE m.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND o.status='completed'
        AND m.movement_kind IN ('material_issue','material_return','labor') AND j.posted_at>=? AND j.posted_at<?
      GROUP BY 1${groupKey} ORDER BY 1`;
        values = [f.currency, a.companyId, a.branchId, a.branchId, Date.parse(f.from), Date.parse(f.to) + 86400000];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: (group, reported) => readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
