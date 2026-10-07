import {commerceInventoryConsumptionSourcesStockEffects} from '../../inventory/public/commerce-stock-owner-effects';
import { commercialOwnerProjectionSources as inventoryProjectionSources } from "../../inventory/public/commercial-owner-projection-sources";
import { inventoryReadProjection as inventoryinventoryProjection } from '../../inventory/public/inventory-read-projections';
import type { SqlDatabase } from '../../runtime/public/runtime/types';
import { assertion } from '../../workflow/public/commands';
export function inventoryConsumptionSources(db: SqlDatabase) {
    return { async availableStock(companyId: string, branchId: string) {
            return (await db.prepare(`SELECT w.id warehouse_id,w.name warehouse_name,p.id product_id,p.sku,p.name product_name,p.unit,p.tracking_mode,
  GREATEST(0,s.quantity_milli-${inventoryinventoryProjection.reservedStock}
  -${inventoryinventoryProjection.quarantinedStock}) available_milli
  FROM (${inventoryProjectionSources.commerce_stock}) s JOIN commerce_warehouses w ON w.id=s.warehouse_id JOIN commerce_products p ON p.id=s.product_id
  WHERE w.company_id=? AND p.company_id=w.company_id AND w.branch_id=? AND w.status='active' AND p.status='active' AND s.quantity_milli>0
  ORDER BY w.name,p.sku LIMIT 200`).bind(companyId, branchId).all<{
                warehouse_id: string;
                warehouse_name: string;
                product_id: string;
                sku: string;
                product_name: string;
                unit: string;
                tracking_mode: string;
                available_milli: number;
            }>()).results;
        },
        async serviceStock(companyId: string, warehouseId: string, productId: string, branchId: string) {
            return db.prepare(`SELECT s.*,p.tracking_mode,w.branch_id
  FROM (${inventoryProjectionSources.commerce_stock}) s JOIN commerce_products p ON p.id=s.product_id AND p.company_id=? JOIN commerce_warehouses w ON w.id=s.warehouse_id AND w.company_id=p.company_id
  WHERE s.warehouse_id=? AND s.product_id=? AND w.branch_id=? AND p.status='active' AND w.status='active'`).bind(companyId, warehouseId, productId, branchId)
                .first<{
                quantity_milli: number;
                value_minor: number;
                version: number;
                tracking_mode: string;
                branch_id: string;
            }>();
        },
        lockStock(warehouseId: string, productId: string) { return db.prepare(`SELECT product_id FROM (${inventoryProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=? FOR UPDATE`).bind(warehouseId, productId); },
        guardStockVersion(key: string, value: "part-stock-version", warehouseId: string, productId: string, version: number) { return assertion(db, key, value, `EXISTS(SELECT 1 FROM (${inventoryProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=? AND version=?)`, [warehouseId, productId, version]); },
        guardAvailableStock(key: string, value: "part-source-availability", warehouseId: string, productId: string, value2: number, quantityMilli: number) {
            return assertion(db, key, value, `EXISTS(SELECT 1 FROM (${inventoryProjectionSources.commerce_stock}) s WHERE s.warehouse_id=? AND s.product_id=? AND s.quantity_milli-
    ${inventoryinventoryProjection.reservedStock}
    -${inventoryinventoryProjection.quarantinedStock}+?>=?)`, [warehouseId, productId, value2, quantityMilli]);
        },
        prepareStockConsumption(quantityMilli: number, value: number, warehouseId: string, productId: string) {
            return commerceInventoryConsumptionSourcesStockEffects(db).prepareStockConsumption(quantityMilli, value, warehouseId, productId);
        } };
}
