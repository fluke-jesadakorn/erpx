import "server-only";

import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {Row} from '@erpx/server-commerce/public/commerce';
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";

import {ApiError} from "@erpx/server-runtime/public/http";

import type {ManufacturingMaterialAllocation,ManufacturingOutputAllocation} from '@erpx/server-manufacturing/domain/lots';export type {ManufacturingMaterialAllocation,ManufacturingOutputAllocation} from '@erpx/server-manufacturing/domain/lots';import {BusinessRuleError} from '@erpx/server-shared/index';import {createManufacturingLotApplication,type ManufacturingLotRepository} from '@erpx/server-manufacturing/application/lots';import {manufacturingLotSources} from '@erpx/server-inventory/public/manufacturing-lot-sources';async function exposed<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
function application(db:SqlDatabase){const repository:ManufacturingLotRepository<SqlStatement>={prepareMaterialLotIssue(value:string,orderId:string,orderVersion:number,productId:string,lotId:string,quantity:number,value2:number,userId:string,now:number){return db.prepare("INSERT INTO manufacturing_material_lot_issues VALUES(?,?,?,?,?,?,?,?,?)").bind(value, orderId, orderVersion, productId, lotId, quantity, value2, userId, now);},
async materialLotIssues(orderId:string){return db.prepare("SELECT * FROM manufacturing_material_lot_issues WHERE order_id=? ORDER BY id").bind(orderId).all<Row>();},
prepareOutputEvidence(outputId:string,orderId:string,orderVersion:number,lotId:string,quantity:number,value:number,userId:string,now:number){return db.prepare("INSERT INTO manufacturing_output_lots VALUES(?,?,?,?,?,?,?,?)").bind(outputId, orderId, orderVersion, lotId, quantity, value, userId, now);},
prepareGenealogy(value:string,orderId:string,id:string | number | null,outputId:string,inputQty:number,orderVersion:number,now:number){return db.prepare("INSERT INTO manufacturing_lot_genealogy VALUES(?,?,?,?,?,?,?)").bind(value, orderId, id, outputId, inputQty, orderVersion, now);},...manufacturingLotSources(db)};return createManufacturingLotApplication({repository,clock:{now:()=>Date.now()}});}export async function prepareManufacturingMaterialLots(db: SqlDatabase,a: WorkContext,input: {
  key: string; orderId: string; orderVersion: number; warehouseId: string; productId: string; trackingMode: string;
  requiredMilli: number; entryDate: string; allocations: ManufacturingMaterialAllocation[];
}){return exposed(()=>application(db).prepareManufacturingMaterialLots(a,input));}
export async function prepareManufacturingOutputLots(db: SqlDatabase,a: WorkContext,input: {
  key: string; orderId: string; orderVersion: number; warehouseId: string; productId: string; trackingMode: string;
  outputMilli: number; totalMinor: number; entryDate: string; allocations: ManufacturingOutputAllocation[];
}){return exposed(()=>application(db).prepareManufacturingOutputLots(a,input));}
