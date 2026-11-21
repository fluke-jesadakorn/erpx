/**
 * Read-only warehouse-transfer compatibility surface.
 *
 * Transfer create/revision, submit, post, reject and cancel now belong to the
 * native command mount (`packages/server-inventory/src/public/transfers.rs` +
 * composition); every write entry point fails closed with
 * `NATIVE_INVENTORY_TRANSFER_REQUIRED`. This module keeps only the current
 * Transfer reads, options and pure permission helpers so workplaces,
 * projections, flow access and records can list and open a Transfer.
 */
import {requireTransferPurchasedRead} from "@erpx/server-inventory/adapters/transfer-purchases";
import {inventoryQualityHoldReadSource} from '@erpx/server-quality/public/inventory-hold-read-source';
import {inventoryReadProjection as inventorycommerceProjection} from '@erpx/server-commerce/public/inventory-read-projections';
import {inventoryReadProjection as inventoryfinanceProjection} from '@erpx/server-finance/public/inventory-read-projections';
import {inventoryReadProjection as inventorypeople_payrollProjection} from '@erpx/server-people_payroll/public/inventory-read-projections';
import {readListPage} from "@erpx/server-work_projection/public/list-page";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {ApiError} from "@erpx/server-runtime/public/http";
import {BusinessRuleError} from '@erpx/server-shared/index';
import {canReadCounts} from '@erpx/server-inventory/domain/counts';
import {transferSchema,transferActionSchema,canPrepareTransfers,canEditTransfer,canReviewTransfer,type TransferRow} from '@erpx/server-inventory/domain/transfers';
import {inventoryTransferSources as commerceSources} from '@erpx/server-commerce/public/inventory-transfers-sources';
import {inventoryTransferSources as workflowSources} from '@erpx/server-workflow/public/inventory-transfers-sources';
export {transferSchema,transferActionSchema,canPrepareTransfers,canEditTransfer,canReviewTransfer} from '@erpx/server-inventory/domain/transfers';
export type {TransferRow} from '@erpx/server-inventory/domain/transfers';
type TransferCommandResult={id:string;status:string;version:number};
const select=`SELECT t.*,s.name source_warehouse_name,d.name target_warehouse_name,chart.base_currency currency FROM inventory_transfers t ${inventorycommerceProjection.transferSource} ${inventorycommerceProjection.transferTarget} ${inventorycommerceProjection.transferSettings} ${inventoryfinanceProjection.transferCurrency}`;
function scope(actor:WorkContext){return {sql:"t.company_id=? AND (?::text IS NULL OR t.source_branch_id=? OR t.target_branch_id=?)",args:[actor.companyId,actor.branchId,actor.branchId,actor.branchId]};}
async function exposed<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
function readable(actor:WorkContext){if(!canReadCounts(actor))throw new BusinessRuleError(403,"INVENTORY_SCOPE_REQUIRED","An inventory or finance assignment is required.");}
async function scopedTransfer(db:SqlDatabase,actor:WorkContext,id:string){const s=scope(actor);return db.prepare(`${select} WHERE t.id=? AND ${s.sql}`).bind(id,...s.args).first<TransferRow>();}
export async function listTransfers(actor:WorkContext){readable(actor);const {write}=await holdingResources(actor.holdingId);await requireTransferPurchasedRead(write,actor);return exposed(async()=>{const s=scope(actor);return (await write.prepare(`${select} WHERE ${s.sql} ORDER BY t.updated_at DESC LIMIT 250`).bind(...s.args).all<TransferRow>()).results;});}
export async function transfersPage(actor:WorkContext,query:{q?:string;status?:string;cursor?:string}){readable(actor);const {write}=await holdingResources(actor.holdingId);await requireTransferPurchasedRead(write,actor);return exposed(async()=>{const s=scope(actor);return readListPage<TransferRow&{id:string;page_key:string}>(write,{sql:`SELECT records.*,updated_at::text AS page_key FROM (${select} WHERE ${s.sql}) records
      WHERE (?='' OR status=?) AND (?='' OR strpos(lower(number||' '||reason||' '||source_warehouse_name||' '||target_warehouse_name),lower(?))>0)`,args:[...s.args,query.status??'',query.status??'',query.q??'',query.q??''],cursor:query.cursor,descending:true,scope:[actor.companyId,actor.branchId,'transfers',query.q??'',query.status??'']});});}
export async function getTransfer(actor:WorkContext,id:string,provided?:SqlDatabase){readable(actor);const write=provided??(await holdingResources(actor.holdingId)).write;await requireTransferPurchasedRead(write,actor,{kind:"transfer",id});return exposed(async()=>{const row=await scopedTransfer(write,actor,id);if(!row)throw new BusinessRuleError(404,"TRANSFER_NOT_FOUND","Transfer not found in this workspace.");return row;});}
export async function transferOptions(actor:WorkContext){requirePermission(actor,"inventory.prepare");readable(actor);const {write}=await holdingResources(actor.holdingId);await requireTransferPurchasedRead(write,actor);return exposed(async()=>{const commerce=commerceSources(write);const[warehouses,stock,lots]=await Promise.all([commerce.activeWarehouses(actor.companyId),commerce.availableStock(actor.companyId,actor.branchId,actor.branchId),write.prepare(`SELECT l.*,b.warehouse_id,b.quantity_milli,b.value_minor,b.version balance_version,p.unit,CASE WHEN EXISTS(SELECT 1 ${inventoryQualityHoldReadSource()} WHERE h.lot_id=l.id AND h.warehouse_id=b.warehouse_id AND h.status NOT IN ('released','cancelled','disposed')) THEN 1 ELSE 0 END quality_held FROM inventory_lots l JOIN inventory_lot_balances b ON b.lot_id=l.id ${inventorycommerceProjection.lineProduct} ${inventorycommerceProjection.lotWarehouse} WHERE l.company_id=? AND b.quantity_milli>0 AND (?::text IS NULL OR w.branch_id=?) ORDER BY l.expiry_date NULLS LAST,l.created_at,l.id`).bind(actor.companyId,actor.branchId,actor.branchId).all<TransferRow>()]);return {warehouses:warehouses.results,sources:warehouses.results.filter(w=>!actor.branchId||actor.branchId===w.branch_id),stock:stock.results,lots:lots.results};});}
export async function transferDetail(actor:WorkContext,id:string){return exposed(async()=>{const transfer=await getTransfer(actor,id);const {write}=await holdingResources(actor.holdingId);const workflow=workflowSources(write);const[lines,allocations,decisions,history]=await Promise.all([
 write.prepare(`SELECT l.*,p.sku,p.tracking_mode,COALESCE(s.version,0) current_version FROM inventory_transfer_lines l ${inventorycommerceProjection.lineProduct} ${inventorycommerceProjection.lineStock} WHERE l.transfer_id=? ORDER BY l.ordinal`).bind(transfer.source_warehouse_id,id).all<TransferRow>(),
 write.prepare("SELECT a.*,l.lot_code,l.serial_number,l.expiry_date,b.quantity_milli current_quantity_milli,b.version current_balance_version FROM inventory_transfer_lot_allocations a JOIN inventory_lots l ON l.id=a.lot_id JOIN inventory_transfer_lines tl ON tl.id=a.transfer_line_id LEFT JOIN inventory_lot_balances b ON b.lot_id=a.lot_id AND b.warehouse_id=? WHERE tl.transfer_id=? ORDER BY tl.ordinal,l.expiry_date NULLS LAST,l.id").bind(transfer.source_warehouse_id,id).all<TransferRow>(),
 write.prepare(`SELECT d.*,COALESCE(p.legal_name,d.actor_id) actor_name FROM inventory_transfer_decisions d ${inventorypeople_payrollProjection.decisionActor} WHERE d.transfer_id=? ORDER BY d.created_at DESC`).bind(actor.companyId,id).all<TransferRow>(),
 workflow.transferHistory(actor.companyId,id)]);return {transfer,lines:lines.results,allocations:allocations.results,decisions:decisions.results,history:history.results};});}
/** Writers are native-only. These retained names fail closed instead of returning to SQL effects. */
export async function saveTransfer(..._input:unknown[]):Promise<TransferCommandResult>{void _input;throw new ApiError(503,'NATIVE_INVENTORY_TRANSFER_REQUIRED','The native warehouse transfer command is required.');}
export async function transitionTransfer(..._input:unknown[]):Promise<TransferCommandResult>{void _input;throw new ApiError(503,'NATIVE_INVENTORY_TRANSFER_REQUIRED','The native warehouse transfer command is required.');}
/** Retained diagnostic shape for engineering fixtures; writes fail closed to native. */
export function inventoryTransferApplication(database:SqlDatabase){return {saveTransfer,transitionTransfer,getTransfer:(actor:WorkContext,id:string)=>getTransfer(actor,id,database),transferDetail:(actor:WorkContext,id:string)=>transferDetail(actor,id)};}
