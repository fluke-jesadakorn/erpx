import {commerceInventoryLandedCostsSourcesStockEffects} from '@erpx/inventory-server/public/commerce-stock-owner-effects';
import { COMMERCE_STOCK_BY_WAREHOUSE_PRODUCT_SQL, COMMERCE_STOCK_SNAPSHOT_GUARD_SQL } from "./stock-source-dedupe-sql";
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {nativeCommerceReadAll,nativeCommerceReadFirst,RD_LANDED_COST_POSTING_CONFIG,RD_LANDED_COST_RECEIPT_LINES} from './native-commerce-read';
import type { LandedCostRow } from '@erpx/inventory-server/public/landed-cost-contract';
export function inventoryLandedCostSources(db: SqlDatabase) {
    return { async receiptLines(fulfillmentId: string) { return { results: await nativeCommerceReadAll<LandedCostRow>(RD_LANDED_COST_RECEIPT_LINES,[fulfillmentId],db) }; },
        async receivingStock(warehouse_id: string | number | null, product_id: string | number | null) { return db.prepare(COMMERCE_STOCK_BY_WAREHOUSE_PRODUCT_SQL).bind(warehouse_id, product_id).first<LandedCostRow>(); },
        guardStockSnapshot(key: string, value: string, warehouse_id: string | number | null, product_id: string | number | null, stock_version: string | number | null, stock_quantity_milli: string | number | null, stock_value_minor: string | number | null) { return assertion(db, key, value, COMMERCE_STOCK_SNAPSHOT_GUARD_SQL, [warehouse_id, product_id, stock_version, stock_quantity_milli, stock_value_minor]); },
        async inventoryPostingConfig(companyId: string) { return nativeCommerceReadFirst<LandedCostRow>(RD_LANDED_COST_POSTING_CONFIG,[companyId],db); },
        prepareLandedStockValue(allocated_minor: string | number | null, warehouse_id: string | number | null, product_id: string | number | null) {
            return commerceInventoryLandedCostsSourcesStockEffects(db).prepareLandedStockValue(allocated_minor, warehouse_id, product_id);
        } };
}
