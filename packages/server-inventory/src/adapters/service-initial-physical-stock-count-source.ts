import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {requirePermission} from '@erpx/server-identity_access/public/iam-permissions';
import {checkPurchasedCapability} from '@erpx/server-commercial/public/purchase-command-boundary';
import {serviceInitialPhysicalStockCommerceLocks} from '@erpx/server-commerce/public/service-initial-physical-stock-sources';
import {initialPhysicalStockCountFinanceLeases} from '@erpx/server-finance/public/initial-physical-stock-count-sources';
import { assertProductionModuleEnabled } from "@erpx/server-pms/public/feature-availability";
import {type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {holdingResources} from '@erpx/server-runtime/public/runtime';
import {serviceInitialPhysicalStockCapabilities,serviceInitialPhysicalStockCountSourceRows,serviceInitialPhysicalStockMarkedRows} from '@erpx/server-inventory/adapters/service-initial-physical-stock-count-source-sql';
export {serviceInitialPhysicalStockCapabilities,serviceInitialPhysicalStockMarker,serviceInitialPhysicalStockCountSourceRows,serviceInitialPhysicalStockMarkedRows,type ServiceInitialPhysicalStockProof} from '@erpx/server-inventory/adapters/service-initial-physical-stock-count-source-sql';
/** Routing hint only: current authorization and full source evidence are rechecked by Rust. */
export async function serviceCountWriterOwner(actor:WorkContext,id:string):Promise<'service'|'other'>{
 const database=(await holdingResources(actor.holdingId)).write;
 return serviceCountWriterOwnerInDatabase(database,actor,id);
}
export async function serviceCountWriterOwnerInDatabase(database:SqlDatabase,actor:WorkContext,id:string):Promise<'service'|'other'>{
 const marked=await serviceCountPersistedMarker(database,id);
 if(!marked)return 'other';
 const native=await database.prepare('SELECT 1 FROM inventory_service_initial_physical_sources WHERE count_id=? AND company_id=?').bind(id,actor.companyId).first();
 if(!native||!await serviceInitialPhysicalStockCountSource(database,actor,id))throw new ApiError(404,'COUNT_NOT_FOUND','Stock count not found in this workspace.');
 return 'service';
}
/** Marker detection deliberately ignores Company so corrupt and foreign markers
 * cannot be mistaken for ordinary legacy-owned counts. */
export async function serviceCountPersistedMarker(database:SqlDatabase,id:string):Promise<boolean>{
 const found=await database.prepare(`SELECT EXISTS(SELECT 1 FROM command_receipts r WHERE r.operation='inventory.count.create' AND r.record_id=? AND (r.result_json IS NULL OR jsonb_typeof(r.result_json::jsonb)<>'object' OR jsonb_exists(r.result_json::jsonb,'_serviceInitialPhysicalStockCount'))) OR EXISTS(SELECT 1 FROM inventory_service_initial_physical_sources WHERE count_id=?) AS marked`).bind(id,id).first<{marked:boolean}>();
 return found?.marked===true;
}
export async function serviceInitialPhysicalStockCountMarked(database:SqlDatabase,id:string){return !!await database.prepare(`SELECT 1 FROM (${serviceInitialPhysicalStockMarkedRows}) marker WHERE id=?`).bind(id).first();}
export async function serviceInitialPhysicalStockCountSource(database:SqlDatabase,scope:{companyId:string;branchId:string|null},id:string){return database.prepare(`SELECT source_proof::text proof FROM (${serviceInitialPhysicalStockCountSourceRows}) source WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?)`).bind(id,scope.companyId,scope.branchId,scope.branchId).first<string>('proof');}
export function serviceInitialPhysicalStockSourceLeases(database:SqlDatabase){return [database.prepare('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE'),database.prepare('LOCK TABLE inventory_ledger,inventory_counts,inventory_count_lines,inventory_count_lot_lines,inventory_count_decisions,inventory_adjustment_moves,inventory_lots,inventory_lot_balances,inventory_lot_moves,inventory_order_reservations,inventory_quality_holds IN SHARE ROW EXCLUSIVE MODE'),...serviceInitialPhysicalStockCommerceLocks(database),...initialPhysicalStockCountFinanceLeases(database)];}
export async function requireServiceInitialPhysicalStockAuthority(database:SqlDatabase,actor:WorkContext,permission='inventory.read'){
 if(actor.departmentId)throw new ApiError(403,'INVENTORY_SCOPE_REQUIRED','Use a current Company or Branch Stock assignment.');
 requirePermission(actor,'inventory.read');requirePermission(actor,permission);
 const current=await requireQueuedCommandAuthority(database,actor,permission);requirePermission(current,'inventory.read');
 for(const moduleId of ['field_service','inventory','finance'] as const)await assertProductionModuleEnabled(database,actor.companyId,moduleId);
 return current;
}
export async function requireServiceInitialPhysicalStockHistory(database:SqlDatabase,actor:WorkContext,id:string){
 await requireServiceInitialPhysicalStockAuthority(database,actor);
 const source=await serviceInitialPhysicalStockCountSource(database,actor,id);if(!source)throw new ApiError(404,'COUNT_NOT_FOUND','The original Initial Physical Stock Count is unavailable.');
 for(const capability of serviceInitialPhysicalStockCapabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose:'history'});return source;
}
