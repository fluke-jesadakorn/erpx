import {commercePmsCommandFactsStockEffects} from '@erpx/server-inventory/public/commerce-stock-owner-effects';
import { commercialOwnerProjectionSources as inventoryProjectionSources } from "@erpx/server-inventory/public/commercial-owner-projection-sources";
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
const query = async <T>(db: SqlDatabase, sql: string, ...values: unknown[]) => (await db
    .prepare(sql)
    .bind(...values)
    .all<T>()).results;
export function pmsCommerceFacts(db: SqlDatabase) {
    return {
        readCommerceProducts: (facts: {
            companyId: string;
        }) => query<{
            id: string;
            name: string;
        }>(db, "SELECT id,name FROM commerce_products WHERE company_id=? AND status='active'", facts.companyId),
        readCommerceWarehouses: (facts: {
            companyId: string;
            fact1: string;
        }) => query<{
            id: string;
            name: string;
        }>(db, "SELECT id,name FROM commerce_warehouses WHERE company_id=? AND branch_id=? AND status='active'", facts.companyId, facts.fact1),
        readCommerceWarehouses2: (facts: {
            warehouseId: string;
            companyId: string;
            branchId: string;
        }) => db.prepare("SELECT id FROM commerce_warehouses WHERE id=? AND company_id=? AND branch_id=? AND status='active'").bind(facts.warehouseId, facts.companyId, facts.branchId).first(),
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
        }) => db.prepare("SELECT 1 FROM commerce_products p JOIN commerce_warehouses w ON w.id=? WHERE p.id=? AND p.company_id=? AND w.company_id=p.company_id AND w.branch_id=? AND p.status='active' AND w.status='active'").bind(facts.warehouseId, facts.productId, facts.companyId, facts.branchId).first()
    };
}
