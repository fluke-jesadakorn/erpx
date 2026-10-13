import { inventoryExecutiveOwnerProjections as commerceProjectionSources } from "../../commerce/public/inventory-executive-owner-projections";
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
    if (id === "inventory") {
        sql = `SELECT w.name label,count(*)::text value FROM commerce_stock s JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=s.warehouse_id WHERE w.company_id=? AND s.quantity_milli>0 GROUP BY w.id,w.name ORDER BY 1`;
        values = [a.companyId];
    }
    if (id === "available-stock") {
        const label = breakdown === "product" ? "p.sku||' · '||p.name"
            : breakdown === "warehouse" ? "w.name"
                : "p.sku||' · '||p.name||' · '||w.name";
        const groupId = breakdown === "product" ? "p.id" : breakdown === "warehouse" ? "w.id" : "p.id||':'||w.id";
        sql = `WITH reservations AS (
        SELECT warehouse_id,product_id,sum(quantity_milli::numeric) quantity_milli
        FROM inventory_active_reservations GROUP BY warehouse_id,product_id
      ), positions AS (
        SELECT w.id warehouse_id,w.name warehouse_name,w.branch_id,p.id product_id,p.sku,p.name product_name,p.unit,
          ${groupId} group_id,${label} group_label,
          GREATEST(s.quantity_milli::numeric-GREATEST(COALESCE(r.quantity_milli,0),inventory_quarantined_quantity(w.id,p.id)::numeric),0) available_milli
        FROM commerce_stock s
        JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=s.warehouse_id AND w.company_id=? AND w.status='active'
        JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=s.product_id AND p.company_id=w.company_id AND p.status='active'
        LEFT JOIN reservations r ON r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id
        WHERE (?::text IS NULL OR w.branch_id=?)
      )
      SELECT min(group_label) label,unit,sum(available_milli)::text value FROM positions
      WHERE available_milli>0
      GROUP BY group_id,unit ORDER BY 1,2 LIMIT 101`;
        values = [a.companyId, a.branchId, a.branchId];
    }
    if (id === "inventory-shortages") {
        const label = breakdown === "warehouse" ? "w.name" : breakdown === "product" ? "p.sku||' · '||p.name" : "'Inventory shortages'";
        const groupId = breakdown === "warehouse" ? "w.id" : breakdown === "product" ? "p.id" : "'all'";
        sql = `WITH reservations AS (
        SELECT warehouse_id,product_id,sum(quantity_milli::numeric) quantity_milli
        FROM inventory_active_reservations GROUP BY warehouse_id,product_id
      ), positions AS (
        SELECT r.id,r.version,w.id warehouse_id,w.name warehouse_name,p.id product_id,p.sku,p.name product_name,p.unit,
          r.current_reorder_point_milli::numeric reorder_point_milli,
          GREATEST(COALESCE(s.quantity_milli,0)::numeric-GREATEST(COALESCE(res.quantity_milli,0),inventory_quarantined_quantity(w.id,p.id)::numeric),0) available_milli,
          ${groupId} group_id,${label} group_label
        FROM inventory_reorder_policies r
        JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=r.warehouse_id AND w.company_id=r.company_id AND w.status='active'
        JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=r.product_id AND p.company_id=r.company_id AND p.status='active'
        LEFT JOIN commerce_stock s ON s.warehouse_id=w.id AND s.product_id=p.id
        LEFT JOIN reservations res ON res.warehouse_id=w.id AND res.product_id=p.id
        WHERE r.company_id=? AND r.current_reorder_point_milli IS NOT NULL AND r.retired_at IS NULL
          AND (?::text IS NULL OR w.branch_id=?)
      )
      SELECT min(group_label) label,count(*) FILTER(WHERE available_milli<reorder_point_milli)::text value,count(*)::text configured_count
      FROM positions GROUP BY group_id ORDER BY label LIMIT 101`;
        values = [a.companyId, a.branchId, a.branchId];
    }
    if (id === "inventory-movements") {
        const label = breakdown === "movement-type" ? "l.source_type"
            : breakdown === "warehouse" ? "w.name"
                : breakdown === "unit" ? "p.unit"
                    : "substring(l.movement_date,1,7)";
        sql = `SELECT ${label} label,p.unit,sum(l.quantity_milli::numeric)::text value
      FROM inventory_ledger l
      JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=l.warehouse_id AND w.company_id=?
      JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=l.product_id AND p.company_id=w.company_id
      WHERE (?::text IS NULL OR w.branch_id=?) AND l.movement_date BETWEEN ? AND ? AND l.quantity_milli<>0
      GROUP BY 1,p.unit ORDER BY 1,2 LIMIT 101`;
        values = [a.companyId, a.branchId, a.branchId, f.from, f.to];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: (group, reported) => readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
