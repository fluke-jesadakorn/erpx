import { platformOwnerProjectionSources as commercePlatformSources } from "@erpx/commerce-server/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as financePlatformSources } from "@erpx/finance-server/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as identity_accessPlatformSources } from "@erpx/identity-access-server/public/platform-owner-projection-sources";
import "server-only";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {holdingResources} from "../../../../../platform/runtime/server/src/public/runtime";
import {requirePermission} from "@erpx/identity-access-server/public/iam-permissions";
import {hasPermission} from "@erpx/identity-access-ui/lib/permissions";
import {getOrder} from '@erpx/commerce-server/public/commerce-order-read';
import {stageSupplierPromiseOrderVersion} from "@erpx/commerce-server/public/supplier-promise-version";
import {nativeSupplierPromiseNotification} from "./native-supplier-promise-notification";
import {nativeProcurementReadAll,PROCUREMENT_RD_PROMISE_HISTORY_BY_ORDER,PROCUREMENT_RD_PROMISE_SCHEDULE_BY_ORDER,PROCUREMENT_RD_OPEN_PROMISE_LINES_BY_ORDER} from "./native-procurement-read";
import {prepareAttachmentBinding} from "@erpx/files-server/public/attachments/service";
import {attachmentPurposeFacts} from "@erpx/files-server/public/attachment-purpose-facts";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/workflow-server/public/commands";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {BusinessRuleError} from "@erpx/platform-shared-server/index";
import {createSupplierPromiseApplication,type SupplierPromisePorts,type PromiseMutation} from "@erpx/procurement-server/application/supplier-promises";
import type {PromiseRow as Row} from "@erpx/procurement-server/domain/supplier-promises";
import {procurementPurchaseAccess,requireProcurementPurchasedRead,withProcurementPurchasedCommand} from "@erpx/procurement-server/adapters/purchases";
import {branchDepartmentScope} from './read-tail-sql';
import {sourceImage} from './read-tail-r31';
import {eligiblePurchaseCount} from './read-tail-r24';
import {openPromiseByOrder} from './read-tail-r41';
import {supplierDetailSelect} from './read-tail-r48';
export {supplierPromiseSchema} from "@erpx/procurement-server/domain/supplier-promises";
type Evidence=Awaited<ReturnType<typeof prepareAttachmentBinding>>;
const scope=branchDepartmentScope("o");
function scopeArgs(a:WorkContext){return [a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId];}
function application(a:WorkContext,provided?:SqlDatabase){
 let resource:Promise<SqlDatabase>|null=null;const database=()=>provided?Promise.resolve(provided):resource??=holdingResources(a.holdingId).then(x=>x.write);
 const ports:SupplierPromisePorts<Evidence>={canRead:hasPermission(a,"commerce.read"),canPurchase:hasPermission(a,"commerce.purchase"),clock:{now:()=>Date.now()},
 async authorizeRead(){requirePermission(a,"commerce.read");},async authorizePurchase(){requirePermission(a,"commerce.purchase");},
 order:async(id)=>{return (await database()).prepare(sourceImage(
    "o.*,p.name supplier_name",
    `(${commercePlatformSources.commerce_orders}) o JOIN (${financePlatformSources.finance_parties}) p ON p.id=o.party_id`,
    `o.id=? AND o.kind='purchase' AND ${scope}`,
  )).bind(id,...scopeArgs(a)).first<Row>();},sourceLines:async(id)=>(await getOrder(a,id,provided)).lines,
 detail:async(id)=>{const write=await database();const [lines,history,schedule]=await Promise.all([
 write.prepare(supplierDetailSelect("l.*,p.sku", `(${commercePlatformSources.commerce_order_lines}) l JOIN (${commercePlatformSources.commerce_products}) p ON p.id=l.product_id WHERE l.order_id=? ORDER BY l.ordinal`)).bind(id).all<Row>(),
 nativeProcurementReadAll<Row>(PROCUREMENT_RD_PROMISE_HISTORY_BY_ORDER,[id],write),
 nativeProcurementReadAll<Row>(PROCUREMENT_RD_PROMISE_SCHEDULE_BY_ORDER,[id],write)]);
 const open=(await nativeProcurementReadAll<Row>(PROCUREMENT_RD_OPEN_PROMISE_LINES_BY_ORDER,[id],write)).results;
return {lines:lines.results,history:history.results,schedule:schedule.results,open};},
 evidenceSets:async(ids)=>attachmentPurposeFacts(await database(),a.companyId,ids),
 prepareEvidence:async(id,promiseId,input,key)=>prepareAttachmentBinding(await database(),a,{attachmentSetIds:input.attachmentSetIds,bindingContract:{targetType:'purchase_order',bindingType:'supplier_promise',contexts:[{purposeKey:'supplier_promise',componentKind:'action',componentRef:'supplier-promise'}]},targetId:id,expectedTargetVersion:input.expectedVersion,commandId:key,bindingId:promiseId}),
 replay:async(key,operation,id,input)=>replayCommand(await database(),a,key,operation,id,input),commit:async(m,result)=>commit(await database(),a,m,result),
 workRows:async()=>{const write=await database();return (await write.prepare(`SELECT o.*,COALESCE(${openPromiseByOrder("min(p.promised_date)"," AND p.remaining_milli>0")},o.due_date) followup_date,
 (SELECT COALESCE(SUM(l.quantity_milli-l.fulfilled_milli),0) FROM (${commercePlatformSources.commerce_order_lines}) l WHERE l.order_id=o.id) outstanding,
 ${openPromiseByOrder("COALESCE(SUM(p.remaining_milli),0)")} promised,
 ${eligiblePurchaseCount(identity_accessPlatformSources.assignments,"a","o.company_id","o.branch_id","o.department_id")} candidates
 FROM (${commercePlatformSources.commerce_orders}) o WHERE o.kind='purchase' AND o.state IN ('confirmed','partially_fulfilled') AND ${scope} ORDER BY o.due_date,o.id`).bind(...scopeArgs(a)).all<Row>()).results;
},};return createSupplierPromiseApplication(ports);
}
/**
 * Retired TypeScript write path. The supplier-promise mutation now executes
 * only in the native Rust command route (`supplier_promise_command_http`),
 * which owns the SQL, the purchase-version CAS, the receipt, the audit event
 * and the outbox in one caller transaction. This adapter keeps the type-level
 * port surface for the retained read use cases but is intentionally fail
 * closed: no SQL is staged here and no TypeScript writer may run as a
 * fallback. Callers must reach the native command transport.
 */
async function commit<Result>(_write:SqlDatabase,_a:WorkContext,_m:PromiseMutation<Evidence>,_result:Result):Promise<Result>{
 throw new ApiError(503,'NATIVE_PROCUREMENT_UNAVAILABLE','The native Procurement command is unavailable.');}
async function invoke<T>(call:()=>Promise<T>):Promise<T>{try{return await call();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
async function readPurchase(a:WorkContext){return requireProcurementPurchasedRead((await holdingResources(a.holdingId)).write,a);}
export const getSupplierPromiseOrder=(a:WorkContext,id:string)=>invoke(async()=>{await readPurchase(a);return application(a).order(id);});
export const supplierPromiseDetail=(a:WorkContext,id:string)=>invoke(async()=>{const access=await readPurchase(a),data=await application(a).detail(id);return {...data,canRecord:data.canRecord&&(access?.settleExisting??true),purchase:access?{newWork:access.newWork,settleExisting:access.settleExisting,history:access.history,revision:access.revision}:undefined};});
/** Fail-closed compatibility export. The native route is authoritative; this
 * symbol is retained only so out-of-scope route modules still typecheck while
 * their cutover is integrated. It never executes a TypeScript writer. */
export const recordSupplierPromise=(_a:WorkContext,_id:string,_raw:unknown,_key:string):Promise<{id:string;promiseId:string;version:number}>=>{
 throw new ApiError(503,'NATIVE_PROCUREMENT_UNAVAILABLE','The native Procurement command is unavailable.');};
export const supplierPromiseWork=(a:WorkContext)=>invoke(async()=>{if(!hasPermission(a,"commerce.read"))return [];const access=await procurementPurchaseAccess((await holdingResources(a.holdingId)).write,a);if(access&&!access.history)return [];const items=await application(a).work();return access&&!access.settleExisting?items.map(item=>({...item,canAct:false,queue:"waiting" as const})):items;});
