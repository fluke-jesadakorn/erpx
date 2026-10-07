import { inventoryExecutiveOwnerProjections as commerceProjectionSources } from "../../commerce/public/inventory-executive-owner-projections";
import "server-only";
export const commerceInventoryPageSources={stock:(branchArgs:Array<string|null>)=>({ sql: `SELECT s.*,s.warehouse_id||':'||s.product_id AS id,p.sku||':'||w.name AS page_key,
      COALESCE((SELECT SUM(r.quantity_milli) FROM inventory_active_reservations r WHERE r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id),0) reserved_milli,
      inventory_quarantined_quantity(s.warehouse_id,s.product_id) quarantined_milli,
      CASE WHEN inventory_quarantined_quantity(s.warehouse_id,s.product_id)>0 THEN 1 ELSE 0 END quality_held,
      COALESCE((SELECT SUM(tl.quantity_milli-tl.received_milli-tl.lost_milli) FROM inventory_transit t JOIN inventory_transit_lines tl ON tl.transit_id=t.id WHERE t.status='in_transit' AND t.target_warehouse_id=s.warehouse_id AND tl.product_id=s.product_id),0) in_transit_milli,
      p.sku,p.name,p.unit,p.tracking_mode,w.name warehouse_name FROM commerce_stock s JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=s.product_id JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=s.warehouse_id
      WHERE w.company_id=? AND (?::text IS NULL OR w.branch_id=?)`, args: branchArgs }),movements:(branchArgs:Array<string|null>,movementCursor:string|undefined,movementScope:Array<string|null>)=>({
            sql: `SELECT m.*,m.created_at::text page_key,p.sku,w.name warehouse_name,m.source_number order_number,m.source_id order_id,m.movement_date fulfillment_date
        FROM inventory_ledger m JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=m.product_id JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=m.warehouse_id
        WHERE w.company_id=? AND (?::text IS NULL OR w.branch_id=?)`, args: branchArgs, descending: true, cursor: movementCursor, scope: movementScope,
        })};
