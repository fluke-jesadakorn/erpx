import {commerceInventoryLandedCostsSourcesStockEffects} from '../../inventory/public/commerce-stock-owner-effects';
import { commercialOwnerProjectionSources as inventoryProjectionSources } from "../../inventory/public/commercial-owner-projection-sources";
import type { SqlDatabase } from '../../runtime/public/runtime/types';
import { assertion } from '../../workflow/public/commands';
import type { LandedCostRow } from '../../inventory/public/landed-cost-contract';
export function inventoryLandedCostSources(db: SqlDatabase) {
    return { async receiptLines(fulfillmentId: string) { return db.prepare("SELECT fl.*,ol.product_id,p.tracking_mode FROM commerce_fulfillment_lines fl JOIN commerce_order_lines ol ON ol.id=fl.order_line_id JOIN commerce_products p ON p.id=ol.product_id WHERE fl.fulfillment_id=? ORDER BY ol.ordinal").bind(fulfillmentId).all<LandedCostRow>(); },
        async receivingStock(warehouse_id: string | number | null, product_id: string | number | null) { return db.prepare(`SELECT * FROM (${inventoryProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=?`).bind(warehouse_id, product_id).first<LandedCostRow>(); },
        guardStockSnapshot(key: string, value: string, warehouse_id: string | number | null, product_id: string | number | null, stock_version: string | number | null, stock_quantity_milli: string | number | null, stock_value_minor: string | number | null) { return assertion(db, key, value, `EXISTS(SELECT 1 FROM (${inventoryProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=? AND version=? AND quantity_milli=? AND value_minor=?)`, [warehouse_id, product_id, stock_version, stock_quantity_milli, stock_value_minor]); },
        async inventoryPostingConfig(companyId: string) { return db.prepare('SELECT chart_id,inventory_account_id FROM commerce_settings WHERE company_id=?').bind(companyId).first<LandedCostRow>(); },
        prepareLandedStockValue(allocated_minor: string | number | null, warehouse_id: string | number | null, product_id: string | number | null) {
            return commerceInventoryLandedCostsSourcesStockEffects(db).prepareLandedStockValue(allocated_minor, warehouse_id, product_id);
        } };
}
