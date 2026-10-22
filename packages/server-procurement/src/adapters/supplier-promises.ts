import { platformOwnerProjectionSources as commercePlatformSources } from "@erpx/server-commerce/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as financePlatformSources } from "@erpx/server-finance/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as identity_accessPlatformSources } from "@erpx/server-identity_access/public/platform-owner-projection-sources";
import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {getOrder} from "@erpx/server-commerce/public/commerce";
import {stageSupplierPromiseOrderVersion} from "@erpx/server-commerce/public/supplier-promise-version";
import {stageSupplierPromiseNotifications} from "@erpx/server-collaboration/public/supplier-promise-notifications";
import {prepareAttachmentBinding} from "@erpx/server-files/public/attachments/service";
import {attachmentPurposeFacts} from "@erpx/server-files/public/attachment-purpose-facts";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {ApiError} from "@erpx/server-runtime/public/http";
import {BusinessRuleError} from "@erpx/server-shared/index";
import {createSupplierPromiseApplication,type SupplierPromisePorts,type PromiseMutation} from "@erpx/server-procurement/application/supplier-promises";
import type {PromiseRow as Row} from "@erpx/server-procurement/domain/supplier-promises";
import {procurementPurchaseAccess,requireProcurementPurchasedRead,withProcurementPurchasedCommand} from "@erpx/server-procurement/adapters/purchases";
export {supplierPromiseSchema} from "@erpx/server-procurement/domain/supplier-promises";
type Evidence=Awaited<ReturnType<typeof prepareAttachmentBinding>>;
const scope="o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)";
function scopeArgs(a:WorkContext){return [a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId];}
function application(a:WorkContext,provided?:SqlDatabase){
 let resource:Promise<SqlDatabase>|null=null;const database=()=>provided?Promise.resolve(provided):resource??=holdingResources(a.holdingId).then(x=>x.write);
 const ports:SupplierPromisePorts<Evidence>={canRead:hasPermission(a,"commerce.read"),canPurchase:hasPermission(a,"commerce.purchase"),clock:{now:()=>Date.now()},
 async authorizeRead(){requirePermission(a,"commerce.read");},async authorizePurchase(){requirePermission(a,"commerce.purchase");},
 order:async(id)=>{return (await database()).prepare(`SELECT o.*,p.name supplier_name FROM (${commercePlatformSources.commerce_orders}) o JOIN (${financePlatformSources.finance_parties}) p ON p.id=o.party_id WHERE o.id=? AND o.kind='purchase' AND ${scope}`).bind(id,...scopeArgs(a)).first<Row>();},sourceLines:async(id)=>(await getOrder(a,id,provided)).lines,
 detail:async(id)=>{const write=await database();const [lines,history,schedule]=await Promise.all([
 write.prepare(`SELECT l.*,p.sku FROM (${commercePlatformSources.commerce_order_lines}) l JOIN (${commercePlatformSources.commerce_products}) p ON p.id=l.product_id WHERE l.order_id=? ORDER BY l.ordinal`).bind(id).all<Row>(),
 write.prepare('SELECT * FROM procurement_delivery_promises WHERE order_id=? ORDER BY order_version DESC').bind(id).all<Row>(),
 write.prepare('SELECT l.* FROM procurement_delivery_promise_lines l JOIN procurement_delivery_promises h ON h.id=l.promise_id WHERE h.order_id=? ORDER BY h.order_version DESC,l.promised_date,l.ordinal').bind(id).all<Row>()]);
 const open=(await write.prepare('SELECT p.* FROM procurement_open_promise_lines p JOIN procurement_delivery_promises h ON h.id=p.promise_id WHERE h.order_id=? ORDER BY p.promised_date,p.ordinal').bind(id).all<Row>()).results;
return {lines:lines.results,history:history.results,schedule:schedule.results,open};},
 evidenceSets:async(ids)=>attachmentPurposeFacts(await database(),a.companyId,ids),
 prepareEvidence:async(id,promiseId,input,key)=>prepareAttachmentBinding(await database(),a,{attachmentSetIds:input.attachmentSetIds,bindingContract:{targetType:'purchase_order',bindingType:'supplier_promise',contexts:[{purposeKey:'supplier_promise',componentKind:'action',componentRef:'supplier-promise'}]},targetId:id,expectedTargetVersion:input.expectedVersion,commandId:key,bindingId:promiseId}),
 replay:async(key,operation,id,input)=>replayCommand(await database(),a,key,operation,id,input),commit:async(m,result)=>commit(await database(),a,m,result),
 workRows:async()=>{const write=await database();return (await write.prepare(`SELECT o.*,COALESCE((SELECT min(p.promised_date) FROM procurement_open_promise_lines p JOIN procurement_delivery_promises h ON h.id=p.promise_id WHERE h.order_id=o.id AND p.remaining_milli>0),o.due_date) followup_date,
 (SELECT COALESCE(SUM(l.quantity_milli-l.fulfilled_milli),0) FROM (${commercePlatformSources.commerce_order_lines}) l WHERE l.order_id=o.id) outstanding,
 (SELECT COALESCE(SUM(p.remaining_milli),0) FROM procurement_open_promise_lines p JOIN procurement_delivery_promises h ON h.id=p.promise_id WHERE h.order_id=o.id) promised,
 (SELECT count(DISTINCT a.user_id) FROM (${identity_accessPlatformSources.assignments}) a WHERE a.company_id=o.company_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'commerce.purchase') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=o.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=o.department_id))) candidates
 FROM (${commercePlatformSources.commerce_orders}) o WHERE o.kind='purchase' AND o.state IN ('confirmed','partially_fulfilled') AND ${scope} ORDER BY o.due_date,o.id`).bind(...scopeArgs(a)).all<Row>()).results;
},};return createSupplierPromiseApplication(ports);
}
async function commit<Result>(write:SqlDatabase,a:WorkContext,m:PromiseMutation<Evidence>,result:Result):Promise<Result>{
 const {key,operation,id,promiseId,input,order,now,evidence:attachmentBinding}=m;
 const prepared=m.lines.map(line=>write.prepare('INSERT INTO procurement_delivery_promise_lines(id,promise_id,order_line_id,ordinal,quantity_milli,fulfilled_snapshot_milli,promised_date) VALUES(?,?,?,?,?,?,?)').bind(`${promiseId}-${line.ordinal}`,promiseId,line.orderLineId,line.ordinal,line.quantityMilli,line.fulfilledSnapshot,line.promisedDate));
 return executeCommand(write,a,key,operation,input,id,input.expectedVersion+1,result,[
 ...(attachmentBinding?.guards??[]),
 assertion(write,key,'source-order',`EXISTS(SELECT 1 FROM (${commercePlatformSources.commerce_orders}) o WHERE o.id=? AND o.version=? AND o.kind='purchase' AND o.state IN ('confirmed','partially_fulfilled') AND `+scope+')',[id,input.expectedVersion,...scopeArgs(a)]),
 write.prepare('INSERT INTO procurement_delivery_promises(id,order_id,company_id,order_version,supplier_id,currency,required_date,kind,confirmation_date,supplier_reference,evidence,recorded_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)').bind(promiseId,id,a.companyId,input.expectedVersion,String(order.party_id),String(order.currency),String(order.due_date),input.kind,input.confirmationDate,input.supplierReference,input.evidence,a.userId,now),...prepared,
 stageSupplierPromiseOrderVersion(write,id,now),...(attachmentBinding?.steps??[]),stageSupplierPromiseNotifications(write,key,id)
 ],{companyId:a.companyId,branchId:String(order.branch_id),departmentId:order.department_id?String(order.department_id):null});}
async function invoke<T>(call:()=>Promise<T>):Promise<T>{try{return await call();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
async function readPurchase(a:WorkContext){return requireProcurementPurchasedRead((await holdingResources(a.holdingId)).write,a);}
export const getSupplierPromiseOrder=(a:WorkContext,id:string)=>invoke(async()=>{await readPurchase(a);return application(a).order(id);});
export const supplierPromiseDetail=(a:WorkContext,id:string)=>invoke(async()=>{const access=await readPurchase(a),data=await application(a).detail(id);return {...data,canRecord:data.canRecord&&(access?.settleExisting??true),purchase:access?{newWork:access.newWork,settleExisting:access.settleExisting,history:access.history,revision:access.revision}:undefined};});
export const recordSupplierPromise=(a:WorkContext,id:string,raw:unknown,key:string)=>invoke(()=>withProcurementPurchasedCommand(a,{key,operation:"procurement.promise.record",target:{kind:"purchase",id},purpose:"settle-existing",permissions:["commerce.read","commerce.purchase"]},database=>application(a,database).record(id,raw,key)));
export const supplierPromiseWork=(a:WorkContext)=>invoke(async()=>{if(!hasPermission(a,"commerce.read"))return [];const access=await procurementPurchaseAccess((await holdingResources(a.holdingId)).write,a);if(access&&!access.history)return [];const items=await application(a).work();return access&&!access.settleExisting?items.map(item=>({...item,canAct:false,queue:"waiting" as const})):items;});
