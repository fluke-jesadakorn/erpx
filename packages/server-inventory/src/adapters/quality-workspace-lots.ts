import {inventoryQualityHoldReadSource} from '@erpx/server-quality/public/inventory-hold-read-source';
import {inventoryReadProjection as inventorycommerceProjection} from '@erpx/server-commerce/public/inventory-read-projections';
import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
type Row=Record<string,string|number|null>;
export function qualityWorkspaceLots(write:SqlDatabase,actor:WorkContext){return write.prepare(`SELECT l.*,b.warehouse_id,b.quantity_milli,b.version balance_version,p.name product_name,w.name warehouse_name,CASE WHEN EXISTS(SELECT 1 ${inventoryQualityHoldReadSource()} WHERE h.lot_id=l.id AND h.warehouse_id=b.warehouse_id AND h.status NOT IN ('released','cancelled','disposed')) THEN 1 ELSE 0 END quality_held FROM inventory_lots l JOIN inventory_lot_balances b ON b.lot_id=l.id ${inventorycommerceProjection.lineProduct} ${inventorycommerceProjection.lotWarehouse} WHERE l.company_id=? AND b.quantity_milli>0 AND (?::text IS NULL OR w.branch_id=?) ORDER BY w.name,p.name,l.expiry_date NULLS LAST,l.id`).bind(actor.companyId,actor.branchId,actor.branchId).all<Row>();}
