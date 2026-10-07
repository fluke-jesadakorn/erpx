import { inventoryExecutiveOwnerProjections as commerceProjectionSources } from "../../commerce/public/inventory-executive-owner-projections";
import { inventoryExecutiveOwnerProjections as qualityProjectionSources } from "../../quality/public/inventory-executive-owner-projections";
import "server-only";
import {all} from "../../pos/public/pos-core";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
export function PosReadInventoryFacts(db:SqlDatabase){return {
readInventoryActiveReservations1:(facts:{company:string;branch:string | null;branch2:string | null;})=>all(db,`SELECT s.warehouse_id,s.product_id,s.quantity_milli,COALESCE((SELECT SUM(r.quantity_milli) FROM inventory_active_reservations r WHERE r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id),0) reserved_milli,CASE WHEN EXISTS(SELECT 1 FROM (${qualityProjectionSources.inventory_quality_holds}) h WHERE h.warehouse_id=s.warehouse_id AND h.product_id=s.product_id AND h.status<>'released') THEN 1 ELSE 0 END quality_held FROM commerce_stock s JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=s.warehouse_id WHERE w.company_id=? AND (?::text IS NULL OR w.branch_id=?)`,facts.company,facts.branch,facts.branch2)
};}
