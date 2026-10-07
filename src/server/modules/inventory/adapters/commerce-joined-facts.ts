import { inventoryExecutiveOwnerProjections as commerceProjectionSources } from "../../commerce/public/inventory-executive-owner-projections";
import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {assertion} from "../../workflow/public/commands";
import type {Row} from "../../commerce/public/commerce-contracts";
export function commerceInventoryJoinedFacts(db:SqlDatabase){return {
readCommerceReturnLotAllocations1: (facts: {
            id: string;
        }) => db.prepare(`SELECT a.*,l.lot_code,l.serial_number,l.expiry_date,COALESCE((SELECT SUM(ra.quantity_milli) FROM (${commerceProjectionSources.commerce_return_lot_allocations}) ra JOIN (${commerceProjectionSources.commerce_return_lines}) rl ON rl.id=ra.return_line_id JOIN (${commerceProjectionSources.commerce_returns}) r ON r.id=rl.return_id WHERE ra.fulfillment_allocation_id=a.id AND r.status!='rejected'),0) returned_milli FROM inventory_fulfillment_lot_allocations a JOIN inventory_lots l ON l.id=a.lot_id JOIN (${commerceProjectionSources.commerce_fulfillment_lines}) fl ON fl.id=a.fulfillment_line_id JOIN (${commerceProjectionSources.commerce_fulfillments}) f ON f.id=fl.fulfillment_id WHERE f.order_id=? ORDER BY f.created_at,a.id`).bind(facts.id).all<Row>(),
readCommerceReturnLotAllocations2: (facts: {
            id: string;
        }) => db.prepare(`SELECT a.*,rl.return_id,l.lot_code,l.serial_number FROM (${commerceProjectionSources.commerce_return_lot_allocations}) a JOIN (${commerceProjectionSources.commerce_return_lines}) rl ON rl.id=a.return_line_id JOIN inventory_lots l ON l.id=a.lot_id JOIN (${commerceProjectionSources.commerce_returns}) r ON r.id=rl.return_id WHERE r.order_id=? ORDER BY r.created_at,a.id`).bind(facts.id).all<Row>(),
readCommerceReturnLotAllocations4: (facts: {
            id: string | number | null;
        }) => db.prepare(`SELECT a.*,l.tracking_mode FROM (${commerceProjectionSources.commerce_return_lot_allocations}) a JOIN inventory_lots l ON l.id=a.lot_id WHERE a.return_line_id=? ORDER BY a.id`).bind(facts.id).all<Row>(),
guardRule2: (facts: {
            key: string;
            guardId: string;
            values: (string | number | null)[];
        }) => assertion(db, facts.key, facts.guardId, `EXISTS(SELECT 1 FROM (${commerceProjectionSources.commerce_order_lines}) l WHERE l.id=? AND l.quantity_milli-l.fulfilled_milli-COALESCE((SELECT SUM(r.quantity_milli) FROM inventory_active_reservations r WHERE r.order_line_id=l.id),0)>=?)`, facts.values)
};}
