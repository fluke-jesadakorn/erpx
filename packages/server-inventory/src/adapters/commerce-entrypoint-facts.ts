import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {commerceEntrypointReadSources} from '@erpx/server-commerce/public/entrypoint-facts';
export function commerceEntrypointInventoryFacts(db:SqlDatabase){return {
 lotRecords:(companyId:string,branchId:string|null)=>db.prepare(`SELECT DISTINCT l.id,l.version,l.lot_code,l.serial_number,l.status,p.sku,p.name FROM inventory_lots l JOIN inventory_lot_balances b ON b.lot_id=l.id JOIN (${commerceEntrypointReadSources.warehouses}) w ON w.id=b.warehouse_id JOIN (${commerceEntrypointReadSources.products}) p ON p.id=l.product_id WHERE l.company_id=? AND (?::text IS NULL OR w.branch_id=?) ORDER BY p.sku,l.lot_code NULLS LAST,l.serial_number NULLS LAST,l.id LIMIT 250`).bind(companyId,branchId,branchId).all<Record<string,unknown>>()
};}
