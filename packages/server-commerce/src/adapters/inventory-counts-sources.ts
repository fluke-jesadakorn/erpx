import {commerceInventoryCountsSourcesStockEffects} from '@erpx/server-inventory/public/commerce-stock-owner-effects';
import { commercialOwnerProjectionSources as inventoryProjectionSources } from "@erpx/server-inventory/public/commercial-owner-projection-sources";
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import type { CountRow } from '@erpx/server-inventory/public/count-contract';
export function inventoryCountSources(db: SqlDatabase) {
    return { async activeWarehouses(companyId: string, branchId: string | null, branchId2: string | null) { return db.prepare("SELECT id,name,branch_id FROM commerce_warehouses WHERE company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?) ORDER BY name").bind(companyId, branchId, branchId2).all<CountRow>(); },
        async activeProducts(companyId: string) { return db.prepare("SELECT id,sku,name,unit,tracking_mode FROM commerce_products WHERE company_id=? AND status='active' ORDER BY sku").bind(companyId).all<CountRow>(); },
        async stockOptions(companyId: string, branchId: string | null, branchId2: string | null) { return db.prepare(`SELECT s.* FROM (${inventoryProjectionSources.commerce_stock}) s JOIN commerce_warehouses w ON w.id=s.warehouse_id WHERE w.company_id=? AND (?::text IS NULL OR w.branch_id=?)`).bind(companyId, branchId, branchId2).all<CountRow>(); },
        guardAbsentStock(key: string, value: string, warehouse: string | number | null, product_id: string | number | null) { return assertion(db, key, value, `NOT EXISTS(SELECT 1 FROM (${inventoryProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=?)`, [warehouse, product_id]); },
        guardStockSnapshot(key: string, value: string, warehouse: string | number | null, product_id: string | number | null, snapshot_version: string | number | null, snapshot_quantity_milli: string | number | null, snapshot_value_minor: string | number | null) { return assertion(db, key, value, `EXISTS(SELECT 1 FROM (${inventoryProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=? AND version=? AND quantity_milli=? AND value_minor=?)`, [warehouse, product_id, snapshot_version, snapshot_quantity_milli, snapshot_value_minor]); },
        async activeWarehouse(warehouseId: string, companyId: string, branchId: string | null, branchId2: string | null) { return db.prepare("SELECT * FROM commerce_warehouses WHERE id=? AND company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?)").bind(warehouseId, companyId, branchId, branchId2).first<CountRow>(); },
        async activeProduct(productId: string, companyId: string) { return db.prepare("SELECT * FROM commerce_products WHERE id=? AND company_id=? AND status='active'").bind(productId, companyId).first<CountRow>(); },
        async currentStock(id: string | number | null, productId: string) { return db.prepare(`SELECT * FROM (${inventoryProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=?`).bind(id, productId).first<CountRow>(); },
        guardProductVersion(key: string, value: string, productId: string, version: string | number | null) { return assertion(db, key, value, "EXISTS(SELECT 1 FROM commerce_products WHERE id=? AND version=? AND status='active')", [productId, version]); },
        guardWarehouseVersion(key: string, value: "warehouse", id: string | number | null, version: string | number | null) { return assertion(db, key, value, "EXISTS(SELECT 1 FROM commerce_warehouses WHERE id=? AND version=? AND status='active')", [id, version]); },
        guardWarehouseActive(key: string, value: "warehouse", warehouse_id: string | number | null, companyId: string) { return assertion(db, key, value, "EXISTS(SELECT 1 FROM commerce_warehouses WHERE id=? AND company_id=? AND status='active')", [warehouse_id, companyId]); },
        guardReviewProductVersion(key: string, value: string, product_id: string | number | null, product_version: string | number | null) { return assertion(db, key, value, "EXISTS(SELECT 1 FROM commerce_products WHERE id=? AND version=? AND status='active')", [product_id, product_version]); },
        prepareStock(warehouse_id: string | number | null, product_id: string | number | null) {
            return commerceInventoryCountsSourcesStockEffects(db).prepareStock(warehouse_id, product_id);
        },
        prepareCountedStock(counted_quantity_milli: string | number | null, counted_value_minor: string | number | null, warehouse_id: string | number | null, product_id: string | number | null) {
            return commerceInventoryCountsSourcesStockEffects(db).prepareCountedStock(counted_quantity_milli, counted_value_minor, warehouse_id, product_id);
        } };
}
