/**
 * Retained read-only spare selector for service parts consumption.
 *
 * The parts-consumption command is native. The available-lots read is owned by
 * the native lane 17 axum route (SQL, projection and Company/branch scope);
 * this adapter transports bounded identities only and fails closed when the
 * native owner is unavailable. No SQL remains here.
 */
import {
  readLane17ReplenishmentRows,
  LANE17_RD_SERVICE_SPARES,
} from './native-lane-17/client';
import 'server-only';

import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {BusinessRuleError} from '@erpx/platform-shared-server/index';
import {readAvailableServiceSpares,type InventorySparesRepository} from '@erpx/inventory-server/application/consumption';
import {inventoryConsumptionSources as commerceSources} from '@erpx/commerce-server/public/inventory-consumption-sources';

type SpareLot={lot_id:string;warehouse_id:string;product_id:string;lot_code:string|null;serial_number:string|null;quantity_milli:number;version:number;expiry_date:string|null};
async function exposed<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
function repository(db:SqlDatabase,actor:WorkContext):InventorySparesRepository{
  const stock=commerceSources(db).availableStock;
  return {
    availableStock:stock,
    availableLots:(companyId:string,branchId:string)=>
      readLane17ReplenishmentRows<SpareLot>(LANE17_RD_SERVICE_SPARES,[companyId,branchId],actor),
  };
}
export async function availableServiceSpares(db:SqlDatabase,a:WorkContext,branchId:string){return exposed(()=>readAvailableServiceSpares(repository(db,a),a,branchId));}
