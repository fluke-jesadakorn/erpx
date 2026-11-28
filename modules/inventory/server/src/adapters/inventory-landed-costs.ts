/**
 * Read-only landed-cost compatibility surface.
 *
 * Landed-cost create, approve, reject, cancel and post now belong to the native
 * command mount (`modules/inventory/server/src/public/landed_costs.rs` +
 * composition); every write entry point fails closed with
 * `NATIVE_INVENTORY_LANDED_COST_REQUIRED`. This module keeps the current
 * receipt/detail reads, options and work projection for purchasing and Finance.
 *
 * The read SQL and its row projection are owned by the native lane 17 axum
 * router. This adapter transports bounded identities only and fails closed
 * when the native owner is unavailable; no TypeScript SQL fallback remains.
 */
import {hasPermission} from "@erpx/identity-access-ui/lib/permissions";
import {samePerson} from "@erpx/identity-access-ui/lib/iam";
import {requireLandedCostPurchasedRead} from "@erpx/inventory-server/adapters/landed-cost-purchases";
import {
  readLane17Rows,
  LANE17_RD_LANDED_COST_SCOPED,
  LANE17_RD_LANDED_COSTS_FOR_ORDER,
  LANE17_RD_LANDED_COST_MOVED_STOCK,
  LANE17_RD_LANDED_COST_WORK_ROWS,
} from './native-lane-17/client';
import {nativeInventoryReadAll,INVENTORY_RD_LANDED_COST_DETAIL_LINES,INVENTORY_RD_LANDED_COST_DETAIL_LOTS,INVENTORY_RD_LANDED_COST_DETAIL_EVENTS} from './native-inventory-read';
import "server-only";
import type {WorkItem} from "@erpx/work-projection-ui/lib/work-item";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {holdingResources} from "../../../../../platform/runtime/server/src/public/runtime";
import {requirePermission} from "@erpx/identity-access-server/public/iam-permissions";
import {minorToDecimal} from "@erpx/finance-ui/lib/finance-money";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {BusinessRuleError} from '@erpx/platform-shared-server/index';
import {landedCostCreateSchema,landedCostActionSchema,type LandedCostRow} from '@erpx/inventory-server/domain/landed-costs';
import {inventoryLandedCostSources as financeSources} from '@erpx/finance-server/public/inventory-landed-costs-sources';
export {landedCostCreateSchema,landedCostActionSchema} from '@erpx/inventory-server/domain/landed-costs';
export type {LandedCostRow} from '@erpx/inventory-server/domain/landed-costs';
type LandedCostCommandResult={id:string;status:string;version:number;journalId:string|null};
async function exposed<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
function readable(actor:WorkContext){requirePermission(actor,'finance.read');if(actor.departmentId)throw new BusinessRuleError(403,'FINANCE_SCOPE_REQUIRED','Use a Branch or Company Finance assignment.');}
async function scopedCost(_db:SqlDatabase,id:string,actor:WorkContext,branchId:string|null){const rows=await readLane17Rows<LandedCostRow>(LANE17_RD_LANDED_COST_SCOPED,[id,actor.companyId,branchId,branchId],actor);return rows[0];}
export async function getLandedCost(actor:WorkContext,id:string){readable(actor);const {write}=await holdingResources(actor.holdingId);await requireLandedCostPurchasedRead(write,actor,id);return exposed(async()=>{const cost=await scopedCost(write,id,actor,actor.branchId);if(!cost)throw new BusinessRuleError(404,'LANDED_COST_NOT_FOUND','Landed cost record not found in your scope.');return cost;});}
export async function landedCostDetail(actor:WorkContext,id:string){const cost=await getLandedCost(actor,id);const {write}=await holdingResources(actor.holdingId);return exposed(async()=>{const[lines,lots,events]=await Promise.all([nativeInventoryReadAll<LandedCostRow>(INVENTORY_RD_LANDED_COST_DETAIL_LINES,[id],write),nativeInventoryReadAll<LandedCostRow>(INVENTORY_RD_LANDED_COST_DETAIL_LOTS,[id],write),nativeInventoryReadAll<LandedCostRow>(INVENTORY_RD_LANDED_COST_DETAIL_EVENTS,[id],write)]);return {cost,lines:lines.results,lots:lots.results,events:events.results};});}
export async function landedCostsForOrder(actor:WorkContext,orderId:string){readable(actor);const {write}=await holdingResources(actor.holdingId);await requireLandedCostPurchasedRead(write,actor);return exposed(async()=>readLane17Rows<LandedCostRow>(LANE17_RD_LANDED_COSTS_FOR_ORDER,[orderId,actor.companyId,actor.branchId,actor.branchId],actor));}
export async function landedCostOptions(actor:WorkContext,fulfillmentId:string){requirePermission(actor,'finance.write');readable(actor);const {write}=await holdingResources(actor.holdingId);await requireLandedCostPurchasedRead(write,actor);return exposed(async()=>{const finance=financeSources(write);const fulfillment=await finance.postedReceipt(fulfillmentId,actor.companyId,actor.branchId,actor.branchId);if(!fulfillment)throw new BusinessRuleError(409,'POSTED_PURCHASE_RECEIPT_REQUIRED','Choose a posted purchase receipt in your Finance scope.');const[accounts,moved]=await Promise.all([finance.creditAccounts(actor.companyId,fulfillment.chart_id),readLane17Rows(LANE17_RD_LANDED_COST_MOVED_STOCK,[fulfillmentId,fulfillment.warehouse_id,fulfillment.created_at],actor)]);return {fulfillment,accounts:accounts.results,blocker:moved.length>0?'Stock has left the receiving warehouse after this receipt. Use a reviewed accounting adjustment for any retrospective COGS effect.':null};});}
export async function landedCostWork(actor:WorkContext):Promise<WorkItem[]>{if(actor.departmentId||!hasPermission(actor,"finance.read"))return [];const {write}=await holdingResources(actor.holdingId);const access=await requireLandedCostPurchasedRead(write,actor);if(access&&!access.history)return [];return exposed(async()=>{const rows=await readLane17Rows<LandedCostRow>(LANE17_RD_LANDED_COST_WORK_ROWS,[actor.companyId,actor.branchId,actor.branchId],actor);return rows.map(r=>{const open=['submitted','approved'].includes(String(r.status)),canReview=!actor.branchId&&hasPermission(actor,'finance.approve')&&!samePerson(actor,r.created_by),canAct=open&&canReview;return {id:String(r.id),type:'landed_cost',actionKind:r.status==='submitted'&&canAct?'approval':'work',title:`Landed cost · ${r.number}`,detail:`${r.order_number} · ${r.fulfillment_number} · ${r.currency} ${minorToDecimal(Number(r.amount_minor))}`,href:`/landed-costs/${r.id}`,status:String(r.status),updatedAt:Number(r.updated_at),queue:open?(canAct?'todo':'waiting'):'completed',canAct,responsibleQueue:open?'Independent Company Finance approver':'Finance',nextAction:r.status==='submitted'?'Review allocation evidence and approve or reject':r.status==='approved'?'Post the approved value adjustment and journal':'Review retained accounting evidence',dueAt:Date.parse(String(r.cost_date)+'T23:59:59Z'),relationships:[...(samePerson(actor,r.created_by)?['requested' as const]:[]),...(canAct?['approval' as const]:[]),...(!samePerson(actor,r.created_by)?['team' as const]:[])]} as WorkItem;});});}
/** Writers are native-only. These retained names fail closed instead of returning to SQL effects. */
export async function createLandedCost(..._input:unknown[]):Promise<LandedCostCommandResult>{void _input;throw new ApiError(503,'NATIVE_INVENTORY_LANDED_COST_REQUIRED','The native landed-cost command is required.');}
export async function transitionLandedCost(..._input:unknown[]):Promise<LandedCostCommandResult>{void _input;throw new ApiError(503,'NATIVE_INVENTORY_LANDED_COST_REQUIRED','The native landed-cost command is required.');}
/** Retained diagnostic shape for engineering fixtures; writes fail closed to native. */
export function inventoryLandedCostApplication(database:SqlDatabase){return {createLandedCost,transitionLandedCost,getLandedCost:(actor:WorkContext,id:string)=>getLandedCost(actor,id),landedCostDetail:(actor:WorkContext,id:string)=>landedCostDetail(actor,id),workCosts:(actor:WorkContext)=>landedCostWork(actor)};}
