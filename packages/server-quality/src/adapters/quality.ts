import {nativeCustomerReturnHoldStatements} from './native-customer-return-hold';
import {BusinessRuleError} from "@erpx/server-shared/index";
import 'server-only';
import {nativeQualityHoldDetail,nativeQualityWork} from '../../../feature-quality/src/public/server';
import type {NativeQualityWorkContextHold} from '../../../feature-quality/src/public/server';
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {ApiError} from "@erpx/server-runtime/public/http";

import type {SqlStatement} from '@erpx/server-runtime/public/runtime/types';
import {createQualityApplication,type QualityRepository} from '@erpx/server-quality/application/quality';
import type {QualityRow as Row,ReceiptQualityHold} from '@erpx/server-quality/domain/quality';

/** Native owner read of the retained Quality workspace/detail facts. The
 * projection, source gate, eligibility and purchase history decision live in
 * the Rust owner port (`quality_work.rs`, `quality_read.rs`); this adapter only
 * binds the current request context. No TypeScript SQL or policy remains in the
 * retained `getQualityHold` / `qualityWork` read family. */
function holdRow(h:NativeQualityWorkContextHold):Row{return {
  id:h.id,company_id:h.companyId,branch_id:h.branchId,warehouse_id:h.warehouseId,
  product_id:h.productId,lot_id:h.lotId,reason:h.reason,due_date:h.dueDate,status:h.status,
  created_by:h.createdBy,last_inspector:h.lastInspector,last_result:h.lastResult,
  released_by:h.releasedBy,release_reason:h.releaseReason,released_at:h.releasedAt,
  version:h.version,created_at:h.createdAt,updated_at:h.updatedAt,spec_id:h.specId,
  spec_version:h.specVersion,quantity_scope:h.quantityScope,held_quantity_milli:h.heldQuantityMilli,
  source_type:h.sourceType,source_id:h.sourceId,cancel_requested_by:h.cancelRequestedBy,
  cancel_reason:h.cancelReason,cancelled_by:h.cancelledBy,cancelled_at:h.cancelledAt,
  disposition_kind:h.dispositionKind,disposition_reason:h.dispositionReason,
  disposition_requested_by:h.dispositionRequestedBy,disposition_count_id:h.dispositionCountId,
  disposition_quantity_milli:h.dispositionQuantityMilli,disposed_by:h.disposedBy,
  disposed_at:h.disposedAt,inspection_count:h.inspectionCount,product_name:h.productName,
  product_unit:h.productUnit,tracking_mode:h.trackingMode,warehouse_name:h.warehouseName,
  lot_code:h.lotCode,serial_number:h.serialNumber,expiry_date:h.expiryDate,eligible_count:h.eligibleCount,
};}

function application(actor:WorkContext,provided?:SqlDatabase){let resource:Promise<SqlDatabase>|null=provided?Promise.resolve(provided):null;return createQualityApplication({clock:{now:()=>Date.now()},repository:async()=>repository(await (resource??=holdingResources(actor.holdingId).then(r=>r.write)),actor)});}
function repository(write:SqlDatabase,actor:WorkContext):QualityRepository<SqlStatement[]>{return {
  receiptHold:async(input,now)=>{
    const statements=await nativeCustomerReturnHoldStatements(actor,input,actor.userId,now);
    const steps:SqlStatement[]=[];
    if(statements.specGuard)steps.push(write.prepare(statements.specGuard.sql).bind(...statements.specGuard.values));
    steps.push(write.prepare(statements.hold.sql).bind(...statements.hold.values),write.prepare(statements.notification.sql).bind(...statements.notification.values));
    return steps;
  }
 };}
async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}

/** Native detail_core read. The owner enforces current scope, Company/branch
 * and purchase history; the client verifies the WorkContext snapshot. */
export async function getQualityHold(actor:WorkContext,id:string,_provided?:SqlDatabase){
  const detail=await nativeQualityHoldDetail(actor,id);
  return holdRow(detail.hold);
}
/** Native Work projection. The owner returns the complete WorkItem payload with
 * its source gate already applied; no TypeScript projection is retained. */
export async function qualityWork(actor:WorkContext){
  if(!hasPermission(actor,"quality.read")||actor.departmentId)return [];
  return (await nativeQualityWork(actor)).items;
}
/** Original protective receiving/return effects compose under the full named
 * Commerce owner. Shared quarantine facts never acquire a management gate. */
export function receiptQualityHoldSteps(db:SqlDatabase,actor:WorkContext,input:ReceiptQualityHold){return translated(()=>application(actor,db).receiptQualityHoldSteps(input));}
