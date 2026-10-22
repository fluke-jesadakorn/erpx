import { commerceFinalOwnerProjectionSources as posProjectionSources } from "@erpx/server-pos/public/commerce-final-owner-projection-sources";
import {commercePosCoreFactsStockEffects} from '@erpx/server-inventory/public/commerce-stock-owner-effects';
import { commercialOwnerProjectionSources as inventoryProjectionSources } from "@erpx/server-inventory/public/commercial-owner-projection-sources";
import "server-only";
import { one, all, run } from "@erpx/server-pos/public/pos-core";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function posCoreCommerceFacts(db: SqlDatabase) {
    return {
        readCommerceStock1: (facts: {
            warehouse_id: string | number | boolean | null;
            productId: string;
        }) => one(db, `SELECT * FROM (${inventoryProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=? FOR UPDATE`, facts.warehouse_id, facts.productId),
        readCommerceStock2: (facts: {
            warehouse_id: string | number | boolean | null;
            productId: string;
        }) => one(db, `SELECT * FROM (${inventoryProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=? FOR UPDATE`, facts.warehouse_id, facts.productId),
        writeCommerceStock1: (facts: {
            quantityMilli: number;
            value: number;
            warehouse_id: string | number | boolean | null;
            productId: string;
        }) => commercePosCoreFactsStockEffects(db).writeCommerceStock1(facts),
        readCommerceSettings1: (facts: {
            company: string;
        }) => one(db, 'SELECT * FROM commerce_settings WHERE company_id=? FOR UPDATE', facts.company),
        readCommerceWarehouses1: (facts: {
            warehouseId: string;
            company: string;
            branchId: string;
        }) => one(db, "SELECT id FROM commerce_warehouses WHERE id=? AND company_id=? AND branch_id=? AND status='active' FOR SHARE", facts.warehouseId, facts.company, facts.branchId),
        readCommerceWarehouses2: (facts: {
            company: string;
        }) => all(db, "SELECT id,branch_id,name FROM commerce_warehouses WHERE company_id=? AND status='active' ORDER BY name,id FOR SHARE", facts.company),
        readCommerceSettings2: (facts: {
            company: string;
        }) => one(db, 'SELECT currency FROM commerce_settings WHERE company_id=?', facts.company),
        readCommerceWarehouses3: (facts: {
            warehouseId: string;
        }) => one(db, "SELECT * FROM commerce_warehouses WHERE id=? AND status='active'", facts.warehouseId),
        readCommerceProducts1: (facts: {
            productId: string;
            company: string;
        }) => one(db, "SELECT id FROM commerce_products WHERE id=? AND company_id=? AND status='active'", facts.productId, facts.company),
        readCommerceProducts2: (facts: {
            productId: string;
            company: string;
        }) => one(db, "SELECT id FROM commerce_products WHERE id=? AND company_id=? AND status='active'", facts.productId, facts.company),
        readCommerceStock3: (facts: {
            warehouse_id: string | number | boolean | null;
            product_id: string | number | boolean | null;
        }) => one(db, `SELECT * FROM (${inventoryProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=? FOR UPDATE`, facts.warehouse_id, facts.product_id),
        writeCommerceStock2: (facts: {
            quantity_milli: string | number | boolean | null;
            cost: number;
            warehouse_id: string | number | boolean | null;
            product_id: string | number | boolean | null;
        }) => commercePosCoreFactsStockEffects(db).writeCommerceStock2(facts),
        readCommerceProducts3: (facts: {
            register_id: string | number | boolean | null;
            productId: string;
            company: string;
        }) => one(db, `SELECT p.*,c.kind,c.station,c.recipe_json,c.modifiers_json,c.tax_inclusive,c.version catalog_version FROM commerce_products p JOIN (${posProjectionSources.pos_catalog}) c ON c.product_id=p.id AND c.company_id=p.company_id AND c.enabled=true JOIN (${posProjectionSources.pos_register_catalog}) rc ON rc.product_id=c.product_id AND rc.register_id=? AND rc.enabled=true WHERE p.id=? AND p.company_id=? AND p.status='active'`, facts.register_id, facts.productId, facts.company),
        writeCommerceStock3: (facts: {
            warehouse_id: string | number | boolean | null;
            product_id: string | number | boolean | null;
        }) => run(db, `SELECT product_id FROM (${inventoryProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=? FOR UPDATE`, facts.warehouse_id, facts.product_id),
        writeCommerceStock4: (facts: {
            quantityMilli: number;
            restoredCost: number;
            warehouse_id: string | number | boolean | null;
            product_id: string | number | boolean | null;
        }) => commercePosCoreFactsStockEffects(db).writeCommerceStock4(facts)
    };
}
