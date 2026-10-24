import {commerceProductionCommandSourcesStockEffects} from '@erpx/server-inventory/public/commerce-stock-owner-effects';
import { commercialOwnerProjectionSources as inventoryProjectionSources } from "@erpx/server-inventory/public/commercial-owner-projection-sources";
import { productionReadProjection as productioninventoryProjection } from '@erpx/server-inventory/public/production-read-projections';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type {Row} from '@erpx/server-commerce/public/commerce';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
export function productionCommandSources(db: SqlDatabase) {
    return { async availableStock(warehouseId: string, product_id: string | number | null) {
            return db.prepare(`SELECT s.version, GREATEST(0,quantity_milli-${productioninventoryProjection.quarantinedStock}
        -${productioninventoryProjection.reservedStock}) available
        FROM (${inventoryProjectionSources.commerce_stock}) s WHERE warehouse_id=? AND product_id=?`).bind(warehouseId, product_id).first<{
                available: number;
                version: number;
            }>();
        }, async products(companyId: string) {
            return db
                .prepare("SELECT id,sku,name FROM commerce_products WHERE company_id=? AND status='active' ORDER BY name")
                .bind(companyId)
                .all<Row>();
        },
        async warehouses(companyId: string, branchId: string | null, branchId2: string | null) {
            return db
                .prepare("SELECT id,name,branch_id FROM commerce_warehouses WHERE company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?) ORDER BY name")
                .bind(companyId, branchId, branchId2)
                .all<Row>();
        },
        async demandSources(companyId: string, branchId: string | null, branchId2: string | null) {
            return db.prepare(`SELECT o.id,o.number,o.version,o.due_date,l.product_id,l.quantity_milli-l.fulfilled_milli remaining_milli
          FROM commerce_orders o JOIN commerce_order_lines l ON l.order_id=o.id WHERE o.company_id=? AND o.kind='sales'
          AND o.state IN ('confirmed','partially_fulfilled') AND l.quantity_milli>l.fulfilled_milli AND (?::text IS NULL OR o.branch_id=?) ORDER BY o.due_date,o.number`)
                .bind(companyId, branchId, branchId2).all<Row>();
        },
        guardOutputProduct(key: string, value: "output", outputProductId: string, companyId: string) {
            return assertion(db, key, value, "EXISTS(SELECT 1 FROM commerce_products WHERE id=? AND company_id=? AND status='active')", [outputProductId, companyId]);
        },
        guardMaterialProduct(key: string, value: string, productId: string, companyId: string) {
            return assertion(db, key, value, "EXISTS(SELECT 1 FROM commerce_products WHERE id=? AND company_id=? AND status='active')", [productId, companyId]);
        },
        async activeWarehouse(warehouseId: string, companyId: string, branchId: string | null, branchId2: string | null) {
            return db.prepare("SELECT * FROM commerce_warehouses WHERE id=? AND company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?)")
                .bind(warehouseId, companyId, branchId, branchId2).first<Row>();
        },
        async demandSource(salesOrderId: string, companyId: string, warehouseId: string, value: number, output_product_id: string | number | null) {
            return db.prepare(`SELECT o.id,o.version,l.quantity_milli-l.fulfilled_milli remaining_milli
      FROM commerce_orders o JOIN commerce_order_lines l ON l.order_id=o.id WHERE o.id=? AND o.company_id=? AND o.warehouse_id=?
      AND o.kind='sales' AND o.version=? AND o.state IN ('confirmed','partially_fulfilled') AND l.product_id=? AND l.quantity_milli>l.fulfilled_milli`)
                .bind(salesOrderId, companyId, warehouseId, value, output_product_id).first<Row>();
        },
        async incomingSupply(companyId: string, warehouseId: string, dueDate: string, product_id: string | number | null) {
            return db.prepare(`SELECT o.id,o.version,o.due_date,l.quantity_milli-l.fulfilled_milli incoming_milli
        FROM commerce_orders o JOIN commerce_order_lines l ON l.order_id=o.id WHERE o.company_id=? AND o.warehouse_id=? AND o.kind='purchase'
        AND o.state IN ('confirmed','partially_fulfilled') AND o.due_date<=? AND l.product_id=? AND l.quantity_milli>l.fulfilled_milli ORDER BY o.due_date,o.id`)
                .bind(companyId, warehouseId, dueDate, product_id).all<Row>();
        },
        lockReserveStock(warehouse_id: string | number | null, product_id: string | number | null) { return db.prepare(`SELECT warehouse_id FROM (${inventoryProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=? FOR UPDATE`).bind(warehouse_id, product_id); },
        guardReserveAvailability(key: string, value: string, warehouse_id: string | number | null, product_id: string | number | null, warehouse_id2: string | number | null, product_id2: string | number | null, required: number) {
            return assertion(db, key, value, `COALESCE((SELECT quantity_milli FROM (${inventoryProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=?),0)-${productioninventoryProjection.reservedAvailability}>=?`, [
                warehouse_id,
                product_id,
                warehouse_id2,
                product_id2,
                required,
            ]);
        },
        async materialStock(warehouse_id: string | number | null, product_id: string | number | null) {
            return db
                .prepare(`SELECT * FROM (${inventoryProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=?`)
                .bind(warehouse_id, product_id)
                .first<Row>();
        },
        lockIssueStock(warehouse_id: string | number | null, product_id: string | number | null) { return db.prepare(`SELECT warehouse_id FROM (${inventoryProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=? FOR UPDATE`).bind(warehouse_id, product_id); },
        guardIssueStock(key: string, value: string, warehouse_id: string | number | null, product_id: string | number | null, value2: number, amount: number, value3: number) { return assertion(db, key, value, `EXISTS(SELECT 1 FROM (${inventoryProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=? AND version=? AND quantity_milli>=? AND value_minor>=?)`, [warehouse_id, product_id, value2, amount, value3]); },
        prepareStockIssue(amount: number, value: number, warehouse_id: string | number | null, product_id: string | number | null) {
            return commerceProductionCommandSourcesStockEffects(db).prepareStockIssue(amount, value, warehouse_id, product_id);
        },
        prepareOutputStock(warehouse_id: string | number | null, output_product_id: string | number | null, output: number, total: number) {
            return commerceProductionCommandSourcesStockEffects(db).prepareOutputStock(warehouse_id, output_product_id, output, total);
        } };
}
