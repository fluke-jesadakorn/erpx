import { inventoryExecutiveOwnerProjections as commerceProjectionSources } from "../../commerce/public/inventory-executive-owner-projections";
import { inventoryExecutiveOwnerProjections as financeProjectionSources } from "../../finance/public/inventory-executive-owner-projections";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import "server-only";
import { type Row } from "@/server/modules/commerce/public/commerce";
export function RecordDetailsInventoryOwnedFacts(database: SqlDatabase) {
    return { readRecordSupplementDataRow(id: string, companyId: string, branchId: string | null, branchId2: string | null) { return database.prepare(`SELECT m.*,p.sku,p.name,p.unit,w.name warehouse_name,c.base_currency currency FROM inventory_ledger m JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=m.product_id JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=m.warehouse_id LEFT JOIN (${commerceProjectionSources.commerce_settings}) s ON s.company_id=w.company_id LEFT JOIN (${financeProjectionSources.finance_charts}) c ON c.id=s.chart_id WHERE m.id=? AND w.company_id=? AND (?::text IS NULL OR w.branch_id=?)`).bind(id, companyId, branchId, branchId2).first<Row>(); },
        readRecordSupplementDataRow2(id: string, companyId: string, companyId2: string, branchId: string | null, branchId2: string | null) { return database.prepare(`SELECT l.*,p.sku,p.name,p.unit FROM inventory_lots l JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=l.product_id WHERE l.id=? AND l.company_id=? AND EXISTS(SELECT 1 FROM inventory_lot_balances b JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=b.warehouse_id WHERE b.lot_id=l.id AND w.company_id=? AND (?::text IS NULL OR w.branch_id=?))`).bind(id, companyId, companyId2, branchId, branchId2).first<Row>(); },
        readRecordSupplementDataRows(id: string, companyId: string, branchId: string | null, branchId2: string | null, offset: number) { return database.prepare(`SELECT b.*,w.name warehouse_name FROM inventory_lot_balances b JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=b.warehouse_id WHERE b.lot_id=? AND w.company_id=? AND (?::text IS NULL OR w.branch_id=?) ORDER BY w.name LIMIT 31 OFFSET ?`).bind(id, companyId, branchId, branchId2, offset).all<Row>(); },
        readRecordSupplementDataRow3(id: string, warehouseId: string, companyId: string, branchId: string | null, branchId2: string | null) {
            return database.prepare(`SELECT s.*,p.sku,p.name,p.unit,w.name warehouse_name,
      COALESCE((SELECT SUM(r.quantity_milli) FROM inventory_active_reservations r WHERE r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id),0) reserved_milli,
      inventory_quarantined_quantity(s.warehouse_id,s.product_id) quarantined_milli
      FROM commerce_stock s JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=s.product_id JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=s.warehouse_id
      WHERE s.product_id=? AND s.warehouse_id=? AND w.company_id=? AND (?::text IS NULL OR w.branch_id=?)`).bind(id, warehouseId, companyId, branchId, branchId2).first<Row>();
        },
        readRecordSupplementDataRows2(warehouseId: string, id: string, offset: number) { return database.prepare("SELECT * FROM inventory_ledger WHERE warehouse_id=? AND product_id=? ORDER BY movement_date DESC,id DESC LIMIT 31 OFFSET ?").bind(warehouseId, id, offset).all<Row>(); } };
}
