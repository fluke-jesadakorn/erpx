/**
 * Read-only warehouse-transfer compatibility surface.
 *
 * Transfer create/revision, submit, post, reject and cancel now belong to the
 * native command mount (`packages/server-inventory/src/public/transfers.rs` +
 * composition); every write entry point fails closed with
 * `NATIVE_INVENTORY_TRANSFER_REQUIRED`. Transfer list and detail reads are
 * served only by the mounted native owner routes; this module keeps the current
 * options/detail-assembly reads and pure permission helpers so workplaces,
 * projections, flow access and records can open a Transfer.
 */
import {requireTransferPurchasedRead} from "@erpx/server-inventory/adapters/transfer-purchases";
import {inventoryQualityHoldReadSource} from '@erpx/server-quality/public/inventory-hold-read-source';
import {activeQualityHoldExistsSql,decisionActorRowsSql,lotAllocationRowsHeadSql,lotBalanceRowsHeadSql,countLineStockHeadSql} from './read-tail-sql';
import {inventoryReadProjection as inventorycommerceProjection} from '@erpx/server-commerce/public/inventory-read-projections';
import {inventoryReadProjection as inventorypeople_payrollProjection} from '@erpx/server-people_payroll/public/inventory-read-projections';
import {readListPageRows} from "@erpx/server-work_projection/public/list-page";
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
import {nativeTransferList} from './native-transfer-list-read';
import {nativeTransferDetail} from './native-transfer-detail-read';
export {transferSchema,transferActionSchema,canPrepareTransfers,canEditTransfer,canReviewTransfer} from '@erpx/server-inventory/domain/transfers';
export type {TransferRow} from '@erpx/server-inventory/domain/transfers';
type TransferCommandResult={id:string;status:string;version:number};
async function exposed<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
function readable(actor:WorkContext){if(!canReadCounts(actor))throw new BusinessRuleError(403,"INVENTORY_SCOPE_REQUIRED","An inventory or finance assignment is required.");}
/** The transfer list and detail SQL authorities were deleted; the mounted native owner routes are the only authorities. */
export async function transfersPage(actor:WorkContext,query:{q?:string;status?:string;cursor?:string}){readable(actor);const {write}=await holdingResources(actor.holdingId);await requireTransferPurchasedRead(write,actor);
  const rows=await nativeTransferList(actor),status=query.status??'',q=(query.q??'').toLowerCase();
  const filtered=(rows as (TransferRow&{id:string;page_key:string})[]).filter(row=>(!status||String(row.status)===status)&&(!q||[row.number,row.reason,row.source_warehouse_name,row.target_warehouse_name].map(v=>String(v??'')).join(' ').toLowerCase().includes(q))).map(row=>({...row,id:String(row.id),page_key:String(row.updated_at??'')}));
  return exposed(()=>readListPageRows<TransferRow&{id:string;page_key:string}>({scope:[actor.companyId,actor.branchId,'transfers',query.q??'',query.status??''],cursor:query.cursor,descending:true},async({cursor,descending,limit})=>{
    const ordered=[...filtered].sort((a,b)=>descending?b.page_key.localeCompare(a.page_key)||b.id.localeCompare(a.id):a.page_key.localeCompare(b.page_key)||a.id.localeCompare(b.id));
    const page=cursor?ordered.filter(row=>descending?row.page_key<cursor.key||(row.page_key===cursor.key&&row.id<cursor.id):row.page_key>cursor.key||(row.page_key===cursor.key&&row.id>cursor.id)):ordered;
    return page.slice(0,limit);
  }));}
/** Sole transfer-detail authority: the mounted native owner route executes the
 *  full row statement. Callers (including Workflow flow access) use the native
 *  port without injecting a SQL unit, so no caller can read the detail row off
 *  its own transaction. A missing native target fails closed with 503. */
export async function getTransfer(actor:WorkContext,id:string){readable(actor);const {write}=await holdingResources(actor.holdingId);await requireTransferPurchasedRead(write,actor,{kind:"transfer",id});return exposed(async()=>{const row=await nativeTransferDetail(actor,id);if(!row)throw new BusinessRuleError(404,"TRANSFER_NOT_FOUND","Transfer not found in this workspace.");return row;});}
export async function transferOptions(actor:WorkContext){requirePermission(actor,"inventory.prepare");readable(actor);const {write}=await holdingResources(actor.holdingId);await requireTransferPurchasedRead(write,actor);return exposed(async()=>{const commerce=commerceSources(write);const[warehouses,stock,lots]=await Promise.all([commerce.activeWarehouses(actor.companyId),commerce.availableStock(actor.companyId,actor.branchId,actor.branchId),write.prepare(`${lotBalanceRowsHeadSql}p.unit,CASE WHEN ${activeQualityHoldExistsSql(inventoryQualityHoldReadSource(), "h.lot_id=l.id AND h.warehouse_id=b.warehouse_id AND h.status NOT IN ('released','cancelled','disposed')")} THEN 1 ELSE 0 END quality_held FROM inventory_lots l JOIN inventory_lot_balances b ON b.lot_id=l.id ${inventorycommerceProjection.lineProduct} ${inventorycommerceProjection.lotWarehouse} WHERE l.company_id=? AND b.quantity_milli>0 AND (?::text IS NULL OR w.branch_id=?) ORDER BY l.expiry_date NULLS LAST,l.created_at,l.id`).bind(actor.companyId,actor.branchId,actor.branchId).all<TransferRow>()]);return {warehouses:warehouses.results,sources:warehouses.results.filter(w=>!actor.branchId||actor.branchId===w.branch_id),stock:stock.results,lots:lots.results};});}
export async function transferDetail(actor:WorkContext,id:string){return exposed(async()=>{const transfer=await getTransfer(actor,id);const {write}=await holdingResources(actor.holdingId);const workflow=workflowSources(write);const[lines,allocations,decisions,history]=await Promise.all([
 write.prepare(`${countLineStockHeadSql}COALESCE(s.version,0) current_version FROM inventory_transfer_lines l ${inventorycommerceProjection.lineProduct} ${inventorycommerceProjection.lineStock} WHERE l.transfer_id=? ORDER BY l.ordinal`).bind(transfer.source_warehouse_id,id).all<TransferRow>(),
 write.prepare(`${lotAllocationRowsHeadSql}b.quantity_milli current_quantity_milli,b.version current_balance_version FROM inventory_transfer_lot_allocations a JOIN inventory_lots l ON l.id=a.lot_id JOIN inventory_transfer_lines tl ON tl.id=a.transfer_line_id LEFT JOIN inventory_lot_balances b ON b.lot_id=a.lot_id AND b.warehouse_id=? WHERE tl.transfer_id=? ORDER BY tl.ordinal,l.expiry_date NULLS LAST,l.id`).bind(transfer.source_warehouse_id,id).all<TransferRow>(),
 write.prepare(decisionActorRowsSql('inventory_transfer_decisions','transfer_id',inventorypeople_payrollProjection.decisionActor)).bind(actor.companyId,id).all<TransferRow>(),
 workflow.transferHistory(actor.companyId,id)]);return {transfer,lines:lines.results,allocations:allocations.results,decisions:decisions.results,history:history.results};});}
/** Writers are native-only. These retained names fail closed instead of returning to SQL effects. */
export async function saveTransfer(..._input:unknown[]):Promise<TransferCommandResult>{void _input;throw new ApiError(503,'NATIVE_INVENTORY_TRANSFER_REQUIRED','The native warehouse transfer command is required.');}
export async function transitionTransfer(..._input:unknown[]):Promise<TransferCommandResult>{void _input;throw new ApiError(503,'NATIVE_INVENTORY_TRANSFER_REQUIRED','The native warehouse transfer command is required.');}
/** Retained diagnostic shape for engineering fixtures; writes fail closed to native. */
export function inventoryTransferApplication(database:SqlDatabase){return {saveTransfer,transitionTransfer,getTransfer:(actor:WorkContext,id:string)=>getTransfer(actor,id),transferDetail:(actor:WorkContext,id:string)=>transferDetail(actor,id)};}
