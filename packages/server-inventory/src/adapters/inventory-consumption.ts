import {inventoryQualityHoldReadSource} from '@erpx/server-quality/public/inventory-hold-read-source';
import {inventoryReadProjection as inventorycommerceProjection} from '@erpx/server-commerce/public/inventory-read-projections';
import 'server-only';

import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import {BusinessRuleError} from '@erpx/server-shared/index';
import {readAvailableServiceSpares,type InventorySparesRepository} from '@erpx/server-inventory/application/consumption';
import {inventoryConsumptionSources as commerceSources} from '@erpx/server-commerce/public/inventory-consumption-sources';

/** Retained read-only spare selector. The parts-consumption command is native. */
async function exposed<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
function repository(db:SqlDatabase):InventorySparesRepository{
  const stock=commerceSources(db).availableStock;
  return {availableStock:stock,async availableLots(companyId:string,branchId:string){return (await db.prepare(`SELECT b.lot_id,b.warehouse_id,l.product_id,l.lot_code,l.serial_number,b.quantity_milli,b.version,l.expiry_date
  FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id ${inventorycommerceProjection.lotWarehouse}
  WHERE l.company_id=? AND w.company_id=l.company_id AND w.branch_id=? AND w.status='active' AND l.status='active' AND b.quantity_milli>0
  AND (l.expiry_date IS NULL OR l.expiry_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))
  AND NOT EXISTS(SELECT 1 ${inventoryQualityHoldReadSource()} WHERE h.warehouse_id=b.warehouse_id AND h.lot_id=l.id AND h.status NOT IN ('released','cancelled','disposed'))
  ORDER BY l.expiry_date NULLS LAST,l.lot_code,l.serial_number LIMIT 500`).bind(companyId,branchId).all<{lot_id:string;warehouse_id:string;product_id:string;lot_code:string|null;serial_number:string|null;quantity_milli:number;version:number;expiry_date:string|null}>()).results;}};
}
export async function availableServiceSpares(db:SqlDatabase,a:WorkContext,branchId:string){return exposed(()=>readAvailableServiceSpares(repository(db),a,branchId));}
