import {manufacturingMetricSource} from "@erpx/manufacturing-server/public/metric-purchases";
import { inventoryExecutiveOwnerProjections as commerceProjectionSources } from "@erpx/commerce-server/public/inventory-executive-owner-projections";
import { inventoryExecutiveOwnerProjections as financeProjectionSources } from "@erpx/finance-server/public/inventory-executive-owner-projections";
import { inventoryExecutiveOwnerProjections as organizationProjectionSources } from "@erpx/organization-server/public/inventory-executive-owner-projections";
import { readMetricSources } from '@erpx/platform-runtime-server/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@erpx/platform-shared-ui/lib/executive/canvas";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { specs } from '@erpx/platform-runtime-server/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@erpx/platform-runtime-server/public/executive/metric-projection-contract';
import { manufacturingMetricBreakdownSelect } from './manufacturing-read-tail-r32-sql';
/** Owner-local fixed read fragment: the organization branch display name used
 * by executive metric breakdowns. The completed-orders and production-cost
 * branch breakdowns project the same organization_nodes witness on the caller's
 * own query and snapshot; one definition keeps them from drifting and removes
 * the redundant TypeScript SQL. It stays a live fragment inside its parent
 * query (no fail-closed replacement) and is outside the RD_* read-plan registry. */
const branchDisplayLabel = `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=o.branch_id AND n.company_id=o.company_id),'Branch')`;
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const source=await manufacturingMetricSource(sourceDb,a,id);
    const spec = specs[id];
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (id === "completed-manufacturing-orders" || id === "manufacturing-orders-with-scrap") {
        const scrap = id === "manufacturing-orders-with-scrap";
        const label = breakdown === "branch" ? branchDisplayLabel
            : "to_char(to_timestamp(o.updated_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')";
        sql = manufacturingMetricBreakdownSelect(label, "count(*)::text", ` FROM manufacturing_orders o JOIN (${source.sql}) purchased_production ON purchased_production.id=o.id WHERE o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND o.status='completed' AND o.updated_at>=? AND o.updated_at<? ${scrap ? "AND o.reported_scrap_milli>0" : ""} GROUP BY 1 ORDER BY 1`);
        values = [a.companyId, a.branchId, a.branchId, Date.parse(f.from), Date.parse(f.to) + 86400000];
    }
    if (id === "production-cost") {
        const label = breakdown === "month" ? "to_char(to_timestamp(j.posted_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
            : breakdown === "branch" ? branchDisplayLabel
                : breakdown === "product" ? "p.name"
                    : breakdown === "warehouse" ? "w.name"
                        : "'Posted production costs'";
        const groupKey = breakdown === "product" ? ",p.id" : breakdown === "warehouse" ? ",w.id" : breakdown === "branch" ? ",o.branch_id" : "";
        sql = manufacturingMetricBreakdownSelect(label, "COALESCE(sum(m.amount_minor::numeric),0)::text", `
      FROM manufacturing_cost_movements m
      JOIN manufacturing_orders o ON o.id=m.order_id AND o.company_id=m.company_id
      JOIN (${source.sql}) purchased_production ON purchased_production.id=o.id
      JOIN manufacturing_specifications s ON s.id=o.specification_id
      JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=s.output_product_id AND p.company_id=o.company_id
      JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=o.warehouse_id AND w.company_id=o.company_id
      JOIN (${financeProjectionSources.finance_journals}) j ON j.id=m.journal_id AND j.company_id=m.company_id AND j.status='posted' AND j.currency=?
      JOIN (${commerceProjectionSources.commerce_settings}) cs ON cs.company_id=o.company_id AND cs.currency=j.currency
      WHERE m.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND o.status='completed'
        AND m.movement_kind IN ('material_issue','material_return','labor') AND j.posted_at>=? AND j.posted_at<?
      GROUP BY 1${groupKey} ORDER BY 1`);
        values = [f.currency, a.companyId, a.branchId, a.branchId, Date.parse(f.from), Date.parse(f.to) + 86400000];
    }
    values=[...source.args,...values];
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: async (group, reported) => {await manufacturingMetricSource(sourceDb,a,id);return readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported);}, watermarks: async () => [] };
}
