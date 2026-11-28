import {commerceInventoryCountsSourcesStockEffects} from '@erpx/inventory-server/public/commerce-stock-owner-effects';
import { commercialOwnerProjectionSources as inventoryProjectionSources } from "@erpx/inventory-server/public/commercial-owner-projection-sources";
import { COMMERCE_STOCK_BY_WAREHOUSE_PRODUCT_SQL, COMMERCE_STOCK_ABSENT_GUARD_SQL, COMMERCE_STOCK_SNAPSHOT_GUARD_SQL } from "./stock-source-dedupe-sql";
import { COMMERCE_PRODUCT_ACTIVE_VERSION_GUARD_SQL } from "./guarded-source-dedupe-sql";
import { nativeCommerceReadAll, nativeCommerceReadFirst, RD_COUNT_ACTIVE_PRODUCTS, RD_PRODUCT_ACTIVE, RD_WAREHOUSE_ACTIVE, RD_WAREHOUSES_ACTIVE } from './native-commerce-read';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import type { CountRow } from '@erpx/inventory-server/public/count-contract';
export function inventoryCountSources(db: SqlDatabase) {
    return { async activeWarehouses(companyId: string, branchId: string | null, branchId2: string | null) { return { results: await nativeCommerceReadAll<CountRow>(RD_WAREHOUSES_ACTIVE, [companyId, branchId, branchId2], db) }; },
        async activeProducts(companyId: string) { return { results: await nativeCommerceReadAll<CountRow>(RD_COUNT_ACTIVE_PRODUCTS, [companyId], db) }; },
        async stockOptions(companyId: string, branchId: string | null, branchId2: string | null) { return db.prepare(`SELECT s.* FROM (${inventoryProjectionSources.commerce_stock}) s JOIN commerce_warehouses w ON w.id=s.warehouse_id WHERE w.company_id=? AND (?::text IS NULL OR w.branch_id=?)`).bind(companyId, branchId, branchId2).all<CountRow>(); },
        guardAbsentStock(key: string, value: string, warehouse: string | number | null, product_id: string | number | null) { return assertion(db, key, value, COMMERCE_STOCK_ABSENT_GUARD_SQL, [warehouse, product_id]); },
        guardStockSnapshot(key: string, value: string, warehouse: string | number | null, product_id: string | number | null, snapshot_version: string | number | null, snapshot_quantity_milli: string | number | null, snapshot_value_minor: string | number | null) { return assertion(db, key, value, COMMERCE_STOCK_SNAPSHOT_GUARD_SQL, [warehouse, product_id, snapshot_version, snapshot_quantity_milli, snapshot_value_minor]); },
        async activeWarehouse(warehouseId: string, companyId: string, branchId: string | null, branchId2: string | null) { return nativeCommerceReadFirst<CountRow>(RD_WAREHOUSE_ACTIVE, [warehouseId, companyId, branchId, branchId2], db); },
        async activeProduct(productId: string, companyId: string) { return nativeCommerceReadFirst<CountRow>(RD_PRODUCT_ACTIVE, [productId, companyId], db); },
        async currentStock(id: string | number | null, productId: string) { return db.prepare(COMMERCE_STOCK_BY_WAREHOUSE_PRODUCT_SQL).bind(id, productId).first<CountRow>(); },
        guardProductVersion(key: string, value: string, productId: string, version: string | number | null) { return assertion(db, key, value, COMMERCE_PRODUCT_ACTIVE_VERSION_GUARD_SQL, [productId, version]); },
        guardWarehouseVersion(key: string, value: "warehouse", id: string | number | null, version: string | number | null) { return assertion(db, key, value, "EXISTS(SELECT 1 FROM commerce_warehouses WHERE id=? AND version=? AND status='active')", [id, version]); },
        guardWarehouseActive(key: string, value: "warehouse", warehouse_id: string | number | null, companyId: string) { return assertion(db, key, value, "EXISTS(SELECT 1 FROM commerce_warehouses WHERE id=? AND company_id=? AND status='active')", [warehouse_id, companyId]); },
        guardReviewProductVersion(key: string, value: string, product_id: string | number | null, product_version: string | number | null) { return assertion(db, key, value, COMMERCE_PRODUCT_ACTIVE_VERSION_GUARD_SQL, [product_id, product_version]); },
        prepareStock(warehouse_id: string | number | null, product_id: string | number | null) {
            return commerceInventoryCountsSourcesStockEffects(db).prepareStock(warehouse_id, product_id);
        },
        prepareCountedStock(counted_quantity_milli: string | number | null, counted_value_minor: string | number | null, warehouse_id: string | number | null, product_id: string | number | null) {
            return commerceInventoryCountsSourcesStockEffects(db).prepareCountedStock(counted_quantity_milli, counted_value_minor, warehouse_id, product_id);
        } };
}
