import {commerceInventoryTransitSourcesStockEffects} from '@erpx/server-inventory/public/commerce-stock-owner-effects';
import { COMMERCE_STOCK_BY_WAREHOUSE_PRODUCT_SQL, COMMERCE_STOCK_ABSENT_GUARD_SQL, COMMERCE_STOCK_VERSION_GUARD_SQL, COMMERCE_STOCK_SNAPSHOT_GUARD_SQL } from "./stock-source-dedupe-sql";
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import type { TransferRow as Row } from '@erpx/server-inventory/public/transfer-contract';
export function inventoryTransitSources(db: SqlDatabase) {
    return { guardActiveLocation(key: string, value: string, value2: string | number | null, company_id: string | number | null, value3: string | number | null) { return assertion(db, key, value, "EXISTS(SELECT 1 FROM commerce_warehouses WHERE id=? AND company_id=? AND branch_id=? AND status='active')", [value2, company_id, value3]); },
        guardSourceStock(key: string, value: string, source_warehouse_id: string | number | null, product_id: string | number | null, source_version: string | number | null, source_quantity_milli: string | number | null, source_value_minor: string | number | null) { return assertion(db, key, value, COMMERCE_STOCK_SNAPSHOT_GUARD_SQL, [source_warehouse_id, product_id, source_version, source_quantity_milli, source_value_minor]); },
        guardProductVersion(key: string, value: string, product_id: string | number | null, companyId: string, product_version: string | number | null) { return assertion(db, key, value, "EXISTS(SELECT 1 FROM commerce_products WHERE id=? AND company_id=? AND status='active' AND version=?)", [product_id, companyId, product_version]); },
        prepareDispatchStock(quantity_milli: string | number | null, value_minor: string | number | null, source_warehouse_id: string | number | null, product_id: string | number | null) {
            return commerceInventoryTransitSourcesStockEffects(db).prepareDispatchStock(quantity_milli, value_minor, source_warehouse_id, product_id);
        },
        async targetStock(target_warehouse_id: string | number | null, product_id: string | number | null) { return db.prepare(COMMERCE_STOCK_BY_WAREHOUSE_PRODUCT_SQL).bind(target_warehouse_id, product_id).first<Row>(); },
        guardTargetStockVersion(key: string, value: string, target_warehouse_id: string | number | null, product_id: string | number | null, version: string | number | null) { return assertion(db, key, value, COMMERCE_STOCK_VERSION_GUARD_SQL, [target_warehouse_id, product_id, version]); },
        guardTargetStockAbsent(key: string, value: string, target_warehouse_id: string | number | null, product_id: string | number | null) { return assertion(db, key, value, COMMERCE_STOCK_ABSENT_GUARD_SQL, [target_warehouse_id, product_id]); },
        prepareTargetStock(target_warehouse_id: string | number | null, product_id: string | number | null) {
            return commerceInventoryTransitSourcesStockEffects(db).prepareTargetStock(target_warehouse_id, product_id);
        },
        prepareReceiveStock(qty: number, value: number, target_warehouse_id: string | number | null, product_id: string | number | null) {
            return commerceInventoryTransitSourcesStockEffects(db).prepareReceiveStock(qty, value, target_warehouse_id, product_id);
        } };
}
