import { inventoryExecutiveOwnerProjections as commerceProjectionSources } from "@erpx/server-commerce/public/inventory-executive-owner-projections";
import { inventoryExecutiveOwnerProjections as qualityProjectionSources } from "@erpx/server-quality/public/inventory-executive-owner-projections";
import {reservedStockAggSql,qualityHeldExistsSql} from './read-tail-sql';
import "server-only";
import {all} from '@erpx/server-runtime/public/runtime/sql';
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function PosReadInventoryFacts(db:SqlDatabase){return {
readInventoryActiveReservations1:(facts:{company:string;branch:string | null;branch2:string | null;})=>all(db,`SELECT s.warehouse_id,s.product_id,s.quantity_milli,${reservedStockAggSql('r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id')} reserved_milli,${qualityHeldExistsSql(qualityProjectionSources.inventory_quality_holds, "h.warehouse_id=s.warehouse_id AND h.product_id=s.product_id AND h.status<>'released'")} FROM commerce_stock s JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=s.warehouse_id WHERE w.company_id=? AND (?::text IS NULL OR w.branch_id=?)`,facts.company,facts.branch,facts.branch2)
};}
