/**
 * Read-only generic stock-count compatibility surface.
 *
 * Draft create/revision and every transition now belong to the native command
 * mount (`packages/server-inventory/src/public/counts.rs` + composition); this
 * module keeps only the current Count reads, options and permission helpers so
 * workplaces, projections and records can list and open a Count.
 */
import {serviceInitialPhysicalStockMarkedRows,serviceInitialPhysicalStockCountSourceRows} from '@erpx/server-inventory/adapters/service-initial-physical-stock-count-source-sql';
import {posInitialPhysicalStockMarkedRows,posInitialPhysicalStockCountSourceRows} from '@erpx/server-inventory/adapters/pos-initial-physical-stock-count-source-sql';
import {manufacturingInitialPhysicalStockMarkedRows,manufacturingInitialPhysicalStockCountSourceRows} from '@erpx/server-inventory/adapters/manufacturing-initial-physical-stock-count-source-sql';
import {initialPhysicalStockMarkedRows,initialPhysicalStockCountSourceRows} from '@erpx/server-inventory/adapters/initial-physical-stock-count-source-sql';
import {requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {commercePackageEnforcementMode} from "@erpx/server-commercial/public/purchase-command-boundary";
import {packageClockSql} from "@erpx/server-commercial/public/purchased-capability-sql";
import {countSourcePurchasePredicate} from "@erpx/server-inventory/adapters/count-record-purchases";
import {countSourceAccess} from "@erpx/server-inventory/adapters/count-purchases";
import {inventoryReadProjection as inventorycommerceProjection} from '@erpx/server-commerce/public/inventory-read-projections';
import {inventoryReadProjection as inventoryfinanceProjection} from '@erpx/server-finance/public/inventory-read-projections';
import {inventoryReadProjection as inventorypeople_payrollProjection} from '@erpx/server-people_payroll/public/inventory-read-projections';
import {readListPage} from "@erpx/server-work_projection/public/list-page";

import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {ApiError} from "@erpx/server-runtime/public/http";
import {BusinessRuleError} from '@erpx/server-shared/index';
import type {CountRow} from '@erpx/server-inventory/domain/counts';
import {canReadCounts} from '@erpx/server-inventory/domain/counts';
export {countSchema,countActionSchema,canReadCounts,canPrepareCounts,canApproveCounts,canEditCount,canReviewCount} from '@erpx/server-inventory/domain/counts';
export type {CountRow} from '@erpx/server-inventory/domain/counts';
import {inventoryCountSources as owner0} from '@erpx/server-workflow/public/inventory-counts-sources';
import {inventoryCountSources as owner1} from '@erpx/server-commerce/public/inventory-counts-sources';
import {inventoryCountSources as owner2} from '@erpx/server-finance/public/inventory-counts-sources';
import {requireCountPurchasedRead} from '@erpx/server-inventory/adapters/count-purchases';
async function exposed<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
async function countListScope(db:SqlDatabase,actor:WorkContext){
 const mode=await commercePackageEnforcementMode();await countSourceAccess(db,actor);
 let current=true;try{await requireQueuedCommandAuthority(db,actor,'inventory.read');}catch(error){if(error instanceof ApiError&&(error.status===401||error.status===403))current=false;else throw error;}
 const marked=`EXISTS(SELECT 1 FROM (${initialPhysicalStockMarkedRows} UNION ALL ${manufacturingInitialPhysicalStockMarkedRows} UNION ALL ${posInitialPhysicalStockMarkedRows} UNION ALL ${serviceInitialPhysicalStockMarkedRows}) initial_marker WHERE initial_marker.id=c.id)`;
 const predicate=countSourcePurchasePredicate('c.id','c.company_id','history','count_list_clock.now_ms');
 return `EXISTS(WITH count_list_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) SELECT 1 FROM count_list_clock WHERE CASE WHEN ${marked} THEN ${current?'TRUE':'FALSE'} AND ${predicate} ELSE ${mode==='enforced'?predicate:'TRUE'} END)`;
}
async function requireCountListHistory(db:SqlDatabase,actor:WorkContext){
 const access=await countSourceAccess(db,actor);if(!access||access.history)return;
 const where=await countListScope(db,actor);
 const included=await db.prepare(`SELECT 1 FROM inventory_counts c WHERE c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND ${where} LIMIT 1`).bind(actor.companyId,actor.branchId,actor.branchId).first();
 if(!included)await requireCountPurchasedRead(db,actor);
}
function repository(db:SqlDatabase,context?:WorkContext){
 return {
 async counts(companyId:string,branchId:string | null,branchId2:string | null){const purchase=context?await countListScope(db,context):"TRUE";return db.prepare(`SELECT c.*,w.name warehouse_name,EXISTS(SELECT 1 FROM (${initialPhysicalStockCountSourceRows}) actual_initial WHERE actual_initial.id=c.id AND actual_initial.company_id=c.company_id) initial_physical_source,EXISTS(SELECT 1 FROM (${manufacturingInitialPhysicalStockCountSourceRows}) actual_mfg_initial WHERE actual_mfg_initial.id=c.id AND actual_mfg_initial.company_id=c.company_id) manufacturing_initial_physical_source,EXISTS(SELECT 1 FROM (${posInitialPhysicalStockCountSourceRows}) actual_pos_initial WHERE actual_pos_initial.id=c.id AND actual_pos_initial.company_id=c.company_id) pos_initial_physical_source,EXISTS(SELECT 1 FROM (${serviceInitialPhysicalStockCountSourceRows}) actual_service_initial WHERE actual_service_initial.id=c.id AND actual_service_initial.company_id=c.company_id) service_initial_physical_source FROM inventory_counts c ${inventorycommerceProjection.countWarehouse} WHERE c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND ${purchase} ORDER BY c.updated_at DESC LIMIT 250`).bind(companyId,branchId,branchId2).all<CountRow>();},
 async readPage(actor:WorkContext,query:{q?:string;status?:string;cursor?:string}){const purchase=await countListScope(db,actor);return readListPage<CountRow & {id:string;page_key:string}>(db,{
   sql:`SELECT c.*,w.name warehouse_name,chart.base_currency currency,c.updated_at::text AS page_key,EXISTS(SELECT 1 FROM (${initialPhysicalStockCountSourceRows}) actual_initial WHERE actual_initial.id=c.id AND actual_initial.company_id=c.company_id) initial_physical_source,EXISTS(SELECT 1 FROM (${manufacturingInitialPhysicalStockCountSourceRows}) actual_mfg_initial WHERE actual_mfg_initial.id=c.id AND actual_mfg_initial.company_id=c.company_id) manufacturing_initial_physical_source,EXISTS(SELECT 1 FROM (${posInitialPhysicalStockCountSourceRows}) actual_pos_initial WHERE actual_pos_initial.id=c.id AND actual_pos_initial.company_id=c.company_id) pos_initial_physical_source,EXISTS(SELECT 1 FROM (${serviceInitialPhysicalStockCountSourceRows}) actual_service_initial WHERE actual_service_initial.id=c.id AND actual_service_initial.company_id=c.company_id) service_initial_physical_source FROM inventory_counts c
    ${inventorycommerceProjection.countWarehouse} ${inventorycommerceProjection.countSettings} ${inventoryfinanceProjection.countCurrency}
    WHERE c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?='' OR c.status=?)
    AND (?='' OR strpos(lower(c.number||' '||w.name||' '||c.reason),lower(?))>0) AND ${purchase}`,
   args:[actor.companyId,actor.branchId,actor.branchId,query.status??'',query.status??'',query.q??'',query.q??''],
   cursor:query.cursor,descending:true,scope:[actor.companyId,actor.branchId,'counts',query.q??'',query.status??''],
 });},
 async scopedCount(id:string,companyId:string,branchId:string | null,branchId2:string | null){return db.prepare(`SELECT c.*,w.name warehouse_name,chart.base_currency currency FROM inventory_counts c ${inventorycommerceProjection.countWarehouse} ${inventorycommerceProjection.countSettings} ${inventoryfinanceProjection.countCurrency} WHERE c.id=? AND c.company_id=? AND (?::text IS NULL OR c.branch_id=?)`).bind(id,companyId,branchId,branchId2).first<CountRow>();},
 async countLines(warehouse_id:string | number | null,id:string){return db.prepare(`SELECT l.*,p.sku,p.tracking_mode,s.quantity_milli current_quantity_milli,s.value_minor current_value_minor,COALESCE(s.version,0) current_version FROM inventory_count_lines l ${inventorycommerceProjection.lineProduct} ${inventorycommerceProjection.lineStock} WHERE l.count_id=? ORDER BY l.ordinal`).bind(warehouse_id,id).all<CountRow>();},
 async countLotLines(warehouse_id:string | number | null,id:string){return db.prepare("SELECT cl.*,b.quantity_milli current_quantity_milli,b.value_minor current_value_minor,COALESCE(b.version,0) current_version FROM inventory_count_lot_lines cl JOIN inventory_count_lines l ON l.id=cl.count_line_id LEFT JOIN inventory_lot_balances b ON b.lot_id=cl.result_lot_id AND b.warehouse_id=? WHERE l.count_id=? ORDER BY l.ordinal,cl.id").bind(warehouse_id,id).all<CountRow>();},
 async countDecisions(companyId:string,id:string){return db.prepare(`SELECT d.*,COALESCE(p.legal_name,d.actor_id) actor_name FROM inventory_count_decisions d ${inventorypeople_payrollProjection.decisionActor} WHERE d.count_id=? ORDER BY d.created_at DESC`).bind(companyId,id).all<CountRow>();},
 ...owner0(db),...owner1(db),...owner2(db)};
}
function readable(actor:WorkContext){if(!canReadCounts(actor))throw new BusinessRuleError(403,"INVENTORY_SCOPE_REQUIRED","Use a branch or holding inventory or finance assignment.");}
export async function listCounts(actor:WorkContext){readable(actor);const {write}=await holdingResources(actor.holdingId);await requireCountListHistory(write,actor);return exposed(async()=>(await repository(write,actor).counts(actor.companyId,actor.branchId,actor.branchId)).results);}
export async function countsPage(actor:WorkContext,query:{q?:string;status?:string;cursor?:string}){readable(actor);const {write}=await holdingResources(actor.holdingId);await requireCountListHistory(write,actor);return exposed(()=>repository(write,actor).readPage(actor,query));}
export async function getCount(actor:WorkContext,id:string,provided?:SqlDatabase){readable(actor);const write=provided??(await holdingResources(actor.holdingId)).write;await requireCountPurchasedRead(write,actor,id);const row=await exposed(()=>repository(write).scopedCount(id,actor.companyId,actor.branchId,actor.branchId));if(!row)throw new ApiError(404,"COUNT_NOT_FOUND","Stock count not found in your workspace.");return row;}
export async function countDetail(actor:WorkContext,id:string){const {write}=await holdingResources(actor.holdingId);await requireCountPurchasedRead(write,actor,id);const count=await getCount(actor,id,write);const [lines,lotLines,history,decisions]=await Promise.all([repository(write).countLines(count.warehouse_id,id),repository(write).countLotLines(count.warehouse_id,id),repository(write).countHistory(actor.companyId,id),repository(write).countDecisions(actor.companyId,id)]);return {count,lines:lines.results,lotLines:lotLines.results,history:history.results,decisions:decisions.results};}
export async function countOptions(actor:WorkContext){readable(actor);const {write}=await holdingResources(actor.holdingId);const [warehouses,products,stock,settings]=await Promise.all([repository(write).activeWarehouses(actor.companyId,actor.branchId,actor.branchId),repository(write).activeProducts(actor.companyId),repository(write).stockOptions(actor.companyId,actor.branchId,actor.branchId),repository(write).functionalCurrency(actor.companyId)]);return {warehouses:warehouses.results,products:products.results,stock:stock.results,lots:[],currency:settings?.currency??""};}
/** In-unit reads for owner adapters that already hold the Holding transaction. */
export async function readCountDetail(database:SqlDatabase,actor:WorkContext,id:string){
 const count=await repository(database).scopedCount(id,actor.companyId,actor.branchId,actor.branchId);
 if(!count)throw new ApiError(404,"COUNT_NOT_FOUND","Stock count not found in your workspace.");
 const [lines,lotLines,history,decisions]=await Promise.all([repository(database).countLines(count.warehouse_id,id),repository(database).countLotLines(count.warehouse_id,id),repository(database).countHistory(actor.companyId,id),repository(database).countDecisions(actor.companyId,id)]);
 return {count,lines:lines.results,lotLines:lotLines.results,history:history.results,decisions:decisions.results};
}
export async function readScopedCount(database:SqlDatabase,actor:WorkContext,id:string){
 const count=await repository(database).scopedCount(id,actor.companyId,actor.branchId,actor.branchId);
 if(!count)throw new ApiError(404,"COUNT_NOT_FOUND","Stock count not found in your workspace.");
 return count;
}
/** Native command only: the composition mount owns create/revision/transition. */
type CountCommandResult={id:string;status:string;version:number;journalId?:string|null};
export async function saveCount(..._input:unknown[]):Promise<CountCommandResult>{void _input;throw new ApiError(503,'NATIVE_INVENTORY_COUNT_REQUIRED','The native stock Count command is required.');}
export async function transitionCount(..._input:unknown[]):Promise<CountCommandResult>{void _input;throw new ApiError(503,'NATIVE_INVENTORY_COUNT_REQUIRED','The native stock Count command is required.');}
/** Retained diagnostic shape for engineering fixtures; writes fail closed to native. */
export function inventoryCountApplication(database:SqlDatabase){return {saveCount,transitionCount,countDetail:(actor:WorkContext,id:string)=>readCountDetail(database,actor,id),getCount:(actor:WorkContext,id:string)=>readScopedCount(database,actor,id)};}
