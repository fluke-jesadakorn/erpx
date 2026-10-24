import {inventoryQualityHoldReadSource} from '@erpx/server-quality/public/inventory-hold-read-source';
import {inventoryReadProjection as inventorycommerceProjection} from '@erpx/server-commerce/public/inventory-read-projections';
import 'server-only';

import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {ApiError} from "@erpx/server-runtime/public/http";
import {prepareJournal} from '@erpx/server-finance/public/finance';
import {snapshotMoney} from "@erpx/server-finance/public/company-money";


import {BusinessRuleError} from '@erpx/server-shared/index';import {createInventoryConsumptionApplication,type InventoryConsumptionRepository} from '@erpx/server-inventory/application/consumption';import type {ConsumptionInput} from '@erpx/server-inventory/domain/consumption-input';import {inventoryConsumptionSources as commerceSources} from '@erpx/server-commerce/public/inventory-consumption-sources';import {inventoryConsumptionSources as financeSources} from '@erpx/server-finance/public/inventory-consumption-sources';
export {consumptionLotSchema} from '@erpx/server-inventory/domain/consumption';
async function inward<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof ApiError)throw new BusinessRuleError(error.status,error.code,error.message);throw error;}}
async function exposed<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
function application(db:SqlDatabase){const repository:InventoryConsumptionRepository<SqlStatement>={async availableLots(companyId:string,branchId:string){return (await db.prepare(`SELECT b.lot_id,b.warehouse_id,l.product_id,l.lot_code,l.serial_number,b.quantity_milli,b.version,l.expiry_date
  FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id ${inventorycommerceProjection.lotWarehouse}
  WHERE l.company_id=? AND w.company_id=l.company_id AND w.branch_id=? AND w.status='active' AND l.status='active' AND b.quantity_milli>0
  AND (l.expiry_date IS NULL OR l.expiry_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))
  AND NOT EXISTS(SELECT 1 ${inventoryQualityHoldReadSource()} WHERE h.warehouse_id=b.warehouse_id AND h.lot_id=l.id AND h.status NOT IN ('released','cancelled','disposed'))
  ORDER BY l.expiry_date NULLS LAST,l.lot_code,l.serial_number LIMIT 500`).bind(companyId,branchId).all<{lot_id:string;warehouse_id:string;product_id:string;lot_code:string|null;serial_number:string|null;quantity_milli:number;version:number;expiry_date:string|null}>()).results;},
guardMovementDate(key:string,value:"part-date",warehouseId:string,productId:string,entryDate:string){return assertion(db,key,value,'NOT EXISTS(SELECT 1 FROM inventory_ledger WHERE warehouse_id=? AND product_id=? AND movement_date>?)',[warehouseId,productId,entryDate]);},
async serviceLot(lotId:string,warehouseId:string,productId:string,companyId:string){return db.prepare(`SELECT b.quantity_milli,b.value_minor,b.version,l.tracking_mode,l.expiry_date,l.status
    FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id WHERE b.lot_id=? AND b.warehouse_id=? AND l.product_id=? AND l.company_id=?`)
    .bind(lotId,warehouseId,productId,companyId).first<{quantity_milli:number;value_minor:number;version:number;tracking_mode:string;expiry_date:string|null;status:string}>();},
lockLot(lotId:string,warehouseId:string){return db.prepare('SELECT lot_id FROM inventory_lot_balances WHERE lot_id=? AND warehouse_id=? FOR UPDATE').bind(lotId,warehouseId);},
guardLotAvailability(key:string,value:string,lotId:string,warehouseId:string,balanceVersion:number,quantityMilli:number,entryDate:string){return assertion(db,key,value,`EXISTS(SELECT 1 FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id WHERE b.lot_id=? AND b.warehouse_id=? AND b.version=? AND b.quantity_milli>=?
     AND l.status='active' AND (l.expiry_date IS NULL OR l.expiry_date>=?) AND NOT EXISTS(SELECT 1 ${inventoryQualityHoldReadSource()} WHERE h.lot_id=l.id AND h.warehouse_id=b.warehouse_id AND h.status NOT IN ('released','cancelled','disposed')))`
      ,[lotId,warehouseId,balanceVersion,quantityMilli,entryDate]);},
prepareLotConsumption(quantityMilli:number,cost:number,lotId:string,warehouseId:string){return db.prepare('UPDATE inventory_lot_balances SET quantity_milli=quantity_milli-?,value_minor=value_minor-?,version=version+1 WHERE lot_id=? AND warehouse_id=?').bind(quantityMilli,cost,lotId,warehouseId);},
prepareLotDepletion(lotId:string,lotId2:string){return db.prepare("UPDATE inventory_lots SET status=CASE WHEN NOT EXISTS(SELECT 1 FROM inventory_lot_balances WHERE lot_id=? AND quantity_milli>0) THEN 'depleted' ELSE status END WHERE id=?").bind(lotId,lotId2);},
prepareLotEvidence(value:string,id:string,lotId:string,quantityMilli:number,cost:number,balanceVersion:number){return db.prepare('INSERT INTO operational_inventory_consumption_lots VALUES(?,?,?,?,?,?)').bind(value,id,lotId,quantityMilli,cost,balanceVersion);},
prepareLotMove(value:string,lotId:string,warehouseId:string,sourceKind:"maintenance" | "field_service",id:string,value2:number,value3:number,entryDate:string,userId:string,now:number){return db.prepare('INSERT INTO inventory_lot_moves(id,lot_id,warehouse_id,source_type,source_id,quantity_milli,value_minor,movement_date,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)').bind(value,lotId,warehouseId,sourceKind,id,value2,value3,entryDate,userId,now);},
prepareConsumption(id:string,companyId:string,branchId:string,departmentId:string | null,sourceKind:"maintenance" | "field_service",sourceId:string,sourceVersion:number,warehouseId:string,productId:string,quantityMilli:number,value:number,currency:string,entryDate:string,id2:string,reason:string,userId:string,now:number){return db.prepare('INSERT INTO operational_inventory_consumptions VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)').bind(id,companyId,branchId,departmentId,sourceKind,sourceId,sourceVersion,warehouseId,productId,quantityMilli,value,currency,entryDate,id2,reason,userId,now);},...commerceSources(db),...financeSources(db)};return createInventoryConsumptionApplication({repository,clock:{now:()=>Date.now()},prepareJournal:(...args)=>inward(()=>prepareJournal(db,...args)),snapshotMoney:(...args)=>inward(()=>snapshotMoney(db,...args))});}
export async function availableServiceSpares(db:SqlDatabase,a:WorkContext,branchId:string){return exposed(()=>application(db).availableServiceSpares(a,branchId));}
export async function prepareInventoryConsumption(db:SqlDatabase,a:WorkContext,input:ConsumptionInput<SqlStatement>){return exposed(()=>application(db).prepareInventoryConsumption(a,input));}
