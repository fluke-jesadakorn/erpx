import {qualitySourceAccess,qualityVisibleHolds,requireQualityPurchasedRead} from "@erpx/server-quality/adapters/purchases";
import {nativeCustomerReturnHoldStatements} from './native-customer-return-hold';
import {BusinessRuleError} from "@erpx/server-shared/index";
import 'server-only';
import {headers} from 'next/headers';
import {createNativeQualityReader,type NativeQualityHold} from '@erpx/feature-quality/adapters/server/native-quality';
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {samePerson} from "@erpx/feature-identity_access/lib/iam";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {ApiError} from "@erpx/server-runtime/public/http";
import {sha256} from "@erpx/server-runtime/public/crypto";

import type {SqlStatement} from '@erpx/server-runtime/public/runtime/types';
import {createQualityApplication,type QualityRepository} from '@erpx/server-quality/application/quality';
import type {QualityRow as Row,ReceiptQualityHold} from '@erpx/server-quality/domain/quality';

/** Native owner read of the retained Quality workspace/detail facts. The
 * projection, source gate and eligibility policy live in the Rust owner port
 * (`quality_work.rs`, `quality_read.rs`); this adapter only binds the current
 * request context and maps the returned hold to the retained `QualityRow`
 * shape. No TypeScript SQL remains in this workspace read family. */
function nativeReader(){
 const raw=process.env.ERPX_NATIVE_QUALITY_CONFIG;
 if(raw===undefined)throw new ApiError(503,'NATIVE_QUALITY_READ_UNAVAILABLE','The native Quality reader is unavailable.');
 return createNativeQualityReader(raw);
}
function holdRow(h:NativeQualityHold):Row{return {
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
async function nativeHoldRows(actor:WorkContext,db:SqlDatabase,id?:string):Promise<Row[]>{
 const reader=nativeReader(),incoming=await headers();
 const holds=id===undefined
  ?(await reader.workspace(incoming,actor.holdingSlug)).holds
  :[(await reader.detailCore(incoming,actor.holdingSlug,id)).hold];
 return qualityVisibleHolds(db,actor,holds.map(holdRow));
}

function application(actor:WorkContext,provided?:SqlDatabase){let resource:Promise<SqlDatabase>|null=provided?Promise.resolve(provided):null;return createQualityApplication({departmentScope:Boolean(actor.departmentId),companyScope:!actor.branchId,userId:actor.userId,authorize:permission=>requirePermission(actor,permission),can:permission=>hasPermission(actor,permission),samePerson:id=>samePerson(actor,id),clock:{now:()=>Date.now()},hash:sha256,repository:async()=>repository(await (resource??=holdingResources(actor.holdingId).then(r=>r.write)),actor)});}
function repository(write:SqlDatabase,actor:WorkContext):QualityRepository<SqlStatement[]>{return {
 holds:id=>nativeHoldRows(actor,write,id),
 receiptHold:async(input,now)=>{
   const statements=await nativeCustomerReturnHoldStatements(actor,input,actor.userId,now);
   const steps:SqlStatement[]=[];
   if(statements.specGuard)steps.push(write.prepare(statements.specGuard.sql).bind(...statements.specGuard.values));
   steps.push(write.prepare(statements.hold.sql).bind(...statements.hold.values),write.prepare(statements.notification.sql).bind(...statements.notification.values));
   return steps;
 }
};}
async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export async function getQualityHold(actor:WorkContext,id:string,provided?:SqlDatabase){const write=provided??(await holdingResources(actor.holdingId)).write;await requireQualityPurchasedRead(write,actor,id);return translated(()=>application(actor,write).getQualityHold(id));}
/** Original protective receiving/return effects compose under the full named
 * Commerce owner. Shared quarantine facts never acquire a management gate. */
export function receiptQualityHoldSteps(db:SqlDatabase,actor:WorkContext,input:ReceiptQualityHold){return translated(()=>application(actor,db).receiptQualityHoldSteps(input));}
export async function qualityWork(actor:WorkContext){if(!hasPermission(actor,"quality.read")||actor.departmentId)return [];const {write}=await holdingResources(actor.holdingId),items=await translated(()=>application(actor,write).qualityWork());return Promise.all(items.map(async item=>{const access=await qualitySourceAccess(write,actor,item.id);return !access||access.settleExisting?item:{...item,canAct:false,actionKind:"work" as const,queue:item.queue==="completed"?"completed" as const:"waiting" as const,relationships:(item.relationships??[]).filter(value=>!["assigned","approval"].includes(value))};}));}
