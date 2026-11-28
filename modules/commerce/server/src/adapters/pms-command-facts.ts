import {commercePmsCommandFactsStockEffects} from '@erpx/inventory-server/public/commerce-stock-owner-effects';
import { commercialOwnerProjectionSources as inventoryProjectionSources } from "@erpx/inventory-server/public/commercial-owner-projection-sources";
import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { nativeCommerceReadAll, nativeCommerceReadFirst, RD_WAREHOUSE_BRANCH_ACTIVE, RD_PMS_PRODUCT_OPTIONS, RD_PMS_WAREHOUSE_ID, RD_PMS_PRODUCT_WAREHOUSE_SCOPE } from './native-commerce-read';
export function pmsCommerceFacts(db: SqlDatabase) {
    return {
        readCommerceProducts: (facts: {
            companyId: string;
        }) => nativeCommerceReadAll<{
            id: string;
            name: string;
        }>(RD_PMS_PRODUCT_OPTIONS,[facts.companyId],db),
        readCommerceWarehouses: (facts: {
            companyId: string;
            fact1: string;
        }) => nativeCommerceReadAll<{
            id: string;
            name: string;
        }>(RD_WAREHOUSE_BRANCH_ACTIVE, [facts.companyId, facts.fact1], db),
        readCommerceWarehouses2: (facts: {
            warehouseId: string;
            companyId: string;
            branchId: string;
        }) => nativeCommerceReadFirst<{id:string}>(RD_PMS_WAREHOUSE_ID,[facts.warehouseId,facts.companyId,facts.branchId],db),
        readCommerceStock: (facts: {
            warehouseId: string;
            productId: string;
        }) => db.prepare(`SELECT quantity_milli,value_minor FROM (${inventoryProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=? FOR UPDATE`).bind(facts.warehouseId, facts.productId).first<{
            quantity_milli: number;
            value_minor: number;
        }>(),
        updateCommerceStock: (facts: {
            q: number;
            cost: number;
            warehouseId: string;
            productId: string;
        }) => commercePmsCommandFactsStockEffects(db).updateCommerceStock(facts),
        readCommerceProducts2: (facts: {
            warehouseId: string;
            productId: string;
            companyId: string;
            branchId: string;
        }) => nativeCommerceReadFirst<{ '1': number }>(RD_PMS_PRODUCT_WAREHOUSE_SCOPE,[facts.warehouseId,facts.productId,facts.companyId,facts.branchId],db)
    };
}
