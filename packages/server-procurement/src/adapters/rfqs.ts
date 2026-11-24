import { platformOwnerProjectionSources as commercePlatformSources } from "@erpx/server-commerce/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as financePlatformSources } from "@erpx/server-finance/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as identity_accessPlatformSources } from "@erpx/server-identity_access/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as recordsPlatformSources } from "@erpx/server-records/public/platform-owner-projection-sources";
import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import { nativeProcurementStatement } from '@erpx/feature-procurement/adapters/server/native-procurement-command';
import {samePerson} from "@erpx/feature-identity_access/lib/iam";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {ApiError} from "@erpx/server-runtime/public/http";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {requestQuotationSource,requestVersionGuard} from '@erpx/server-records/public/employee-requests';
import {serviceAuthoritySnapshot} from "@erpx/server-support/public/service-access";
import {prepareAttachmentBinding} from "@erpx/server-files/public/attachments/service";
import {nativeRfqNotification} from "./native-rfq-notification";
import {nativeProcurementReadAll,nativeProcurementReadFirst,PROCUREMENT_RD_RFQ_BY_ORDER,PROCUREMENT_RD_RFQ_OFFER_LINES} from "./native-procurement-read";
import {BusinessRuleError} from "@erpx/server-shared/index";
import {createRfqApplication,type RfqPorts,type RfqMutation} from "@erpx/server-procurement/application/rfqs";
import type {RfqRow as Row,RfqOrderInput} from "@erpx/server-procurement/domain/rfqs";
import {rfqActionSchema} from "@erpx/server-procurement/domain/rfqs";
import {procurementPurchaseAccess,requireProcurementPurchasedRead,withProcurementPurchasedCommand,prepareRfqPurchaseHandoff} from "@erpx/server-procurement/adapters/purchases";
import {branchDepartmentScope,pointRead,orderedRead} from './read-tail-sql';
import {versionStatusScopeGuard,activeMasterLookup} from './read-tail-r30';
import {pointVersionGuard} from './read-tail-r19';
import {eligiblePurchaseCount,memberScopeExclusion} from './read-tail-r24';
import {settingsRead} from './read-tail-r27';
import {approvedRequestExists,rfqScopedRead} from './read-tail-r34';
export {rfqCreateSchema,rfqActionSchema} from "@erpx/server-procurement/domain/rfqs";
type Evidence=Awaited<ReturnType<typeof prepareAttachmentBinding>>;
type Authority=SqlStatement[];
const scope=branchDepartmentScope("r");
const currencyRead=settingsRead("currency",commercePlatformSources.commerce_settings);
const args=(a:WorkContext)=>[a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId];
function guard(db:SqlDatabase,a:WorkContext,r:Row,key:string,version:number){return assertion(db,key,'rfq-version',versionStatusScopeGuard("procurement_rfqs","r",scope),[r.id,version,r.status,...args(a)]);}
function event(db:SqlDatabase,a:WorkContext,id:string,key:string,version:number,action:string,evidence:string,offer:string|null,now:number){return nativeProcurementStatement('procurement_stmt_11', { now: Date.now() }).bind(`rfq-event-${key}`,id,version,action,a.userId,evidence,offer,now);}
function application(a:WorkContext,provided?:SqlDatabase){
 let resource:Promise<SqlDatabase>|null=null;const database=()=>provided?Promise.resolve(provided):resource??=holdingResources(a.holdingId).then(x=>x.write);
 const ports:RfqPorts<Evidence,Authority,SqlStatement>={clock:{now:()=>Date.now()},canRead:hasPermission(a,'commerce.read'),canPurchase:hasPermission(a,'commerce.purchase'),canApprove:hasPermission(a,'commerce.approve'),samePerson:(id)=>samePerson(a,id),
 async authorizeRead(){requirePermission(a,'commerce.read');},async authorizePurchase(){requirePermission(a,'commerce.purchase');},async authorizeApprove(){requirePermission(a,'commerce.approve');},
 replay:async(key,operation,id,input)=>replayCommand(await database(),a,key,operation,id,input),
 rfq:async(id)=>(await database()).prepare(rfqScopedRead(`r.id=? AND ${scope}`)).bind(id,...args(a)).first<Row>(),
 detail:async(id)=>{const write=await database();const [lines,offers,offerLines,events]=await Promise.all([
 write.prepare(orderedRead('*','procurement_rfq_lines','rfq_id=?','ordinal')).bind(id).all<Row>(),
 write.prepare(orderedRead('o.*,p.name supplier_name',`procurement_rfq_offers o JOIN (${financePlatformSources.finance_parties}) p ON p.id=o.supplier_id`,'o.rfq_id=?','o.rfq_version DESC')).bind(id).all<Row>(),
 nativeProcurementReadAll<Row>(PROCUREMENT_RD_RFQ_OFFER_LINES,[id],write),
 write.prepare(orderedRead('*','procurement_rfq_events','rfq_id=?','version DESC')).bind(id).all<Row>()]);return {lines:lines.results,offers:offers.results,offerLines:offerLines.results,events:events.results};},
 workspace:async()=>{const write=await database();const [rfqs,products,warehouses,suppliers,config]=await Promise.all([
 write.prepare(rfqScopedRead(`${scope} ORDER BY r.updated_at DESC LIMIT 100`)).bind(...args(a)).all<Row>(),
 write.prepare(activeMasterLookup('id,sku,name,unit',`${commercePlatformSources.commerce_products}`,'commerce_products','sku')).bind(a.companyId).all<Row>(),
 write.prepare(activeMasterLookup('id,name,branch_id',`${commercePlatformSources.commerce_warehouses}`,'commerce_warehouses','name',''," AND (?::text IS NULL OR branch_id=?)")).bind(a.companyId,a.branchId,a.branchId).all<Row>(),
 write.prepare(activeMasterLookup('id,name',`${financePlatformSources.finance_parties}`,'finance_parties','name'," AND kind='supplier'")).bind(a.companyId).all<Row>(),
 write.prepare(currencyRead).bind(a.companyId).first<Row>()]);return {rfqs:rfqs.results,products:products.results,warehouses:warehouses.results,suppliers:suppliers.results,currency:String(config?.currency??'')};},
 sourceState:async(id)=>(await database()).prepare(pointRead('state,evidence_version,items_json,approval_json',`(${recordsPlatformSources.employee_requests}) employee_requests`)).bind(id,a.companyId).first<Row>(),
 quotationSource:async(id)=>requestQuotationSource(await database(),a,id),currentQuotationSource:async(id)=>{try{return await requestQuotationSource(await database(),a,id);}catch(error){if(error instanceof ApiError&&[403,404,409].includes(error.status))return null;throw error;}},
 configuration:async()=>(await database()).prepare(currencyRead).bind(a.companyId).first<Row>(),
 warehouse:async(id)=>(await database()).prepare(pointRead('*',`(${commercePlatformSources.commerce_warehouses}) commerce_warehouses`,` AND status='active' AND (?::text IS NULL OR branch_id=?)`)).bind(id,a.companyId,a.branchId,a.branchId).first<Row>(),
 product:async(id)=>(await database()).prepare(pointRead('*',`(${commercePlatformSources.commerce_products}) commerce_products`,` AND status='active'`)).bind(id,a.companyId).first<Row>(),
 supplier:async(id,currency)=>(await database()).prepare(pointRead('*',`(${financePlatformSources.finance_parties}) finance_parties`,` AND kind='supplier' AND status='active' AND currency=?`)).bind(id,a.companyId,currency).first<Row>(),
 prepareSource:async(source,key)=>{const write=await database(),snapshot=await serviceAuthoritySnapshot(write,a.companyId);return [...snapshot(key),requestVersionGuard(write,key,source.request)];},
 prepareEvidence:async(id,offer,input,key)=>prepareAttachmentBinding(await database(),a,{attachmentSetIds:input.attachmentSetIds,bindingContract:{targetType:'procurement_rfq',bindingType:'procurement_rfq_offer',contexts:[{purposeKey:'supplier_quotation',componentKind:'action',componentRef:'supplier-offer'}]},targetId:id,expectedTargetVersion:input.expectedVersion,commandId:key,bindingId:offer}),
 commit:async(m,result)=>commit(await database(),a,m,result),
 linkedRfq:async(id)=>nativeProcurementReadFirst<Row>(PROCUREMENT_RD_RFQ_BY_ORDER,[id,a.companyId],await database()),
 stageHandoff:async(h)=>{const db=await database(),r=h.detail.rfq;
 await prepareRfqPurchaseHandoff(db,a,String(r.id),h.orderId,h.key,h.editing);
 const sourceGuard=r.request_id?[assertion(db,h.key,'rfq-source-evidence',approvedRequestExists(`(${recordsPlatformSources.employee_requests}) e JOIN procurement_request_orders l ON l.request_id=e.id AND l.order_id=? AND l.rfq_id=?`,'e.',`e.id=? AND e.company_id=?`,'?','?'),[h.orderId,r.id,r.request_id,a.companyId,r.request_evidence_version,r.request_items_json])]:[];
 const currencyGuard=assertion(db,h.key,'rfq-currency',`EXISTS(SELECT 1 FROM (${commercePlatformSources.commerce_settings}) commerce_settings WHERE company_id=? AND currency=?)`,[a.companyId,r.currency]);
 if(h.editing)return [currencyGuard,...sourceGuard,guard(db,a,r,h.key,Number(r.version))];
 return [currencyGuard,...sourceGuard,guard(db,a,r,h.key,h.order.rfqVersion??0),nativeProcurementStatement('procurement_stmt_12', { now: Date.now() }).bind(h.orderId,h.now,r.id),event(db,a,String(r.id),h.key,Number(r.version)+1,'order-created',`Draft purchase order ${h.orderId} created; normal PO approval still required.`,String(h.selected.id),h.now),await nativeRfqNotification(db,a,String(r.id),h.key)];},
 workRows:async()=>{const write=await database();
return (await write.prepare(`SELECT r.*,o.recorded_by offer_recorder,CASE WHEN r.request_id IS NULL THEN 1 WHEN ${approvedRequestExists(`(${recordsPlatformSources.employee_requests}) source`,'source.',`source.id=r.request_id AND source.company_id=r.company_id AND source.state='in_progress'`,'r.request_evidence_version','r.request_items_json')} THEN 1 ELSE 0 END source_ready,
 ${eligiblePurchaseCount(identity_accessPlatformSources.assignments,"candidate","r.company_id","r.branch_id","r.department_id",` AND (r.status<>'review' OR (iam_role_allows(candidate.role,candidate.company_id,'commerce.approve') AND candidate.user_id<>r.created_by AND candidate.user_id<>COALESCE(r.submitted_by,'') AND candidate.user_id<>COALESCE(o.recorded_by,'') AND ${memberScopeExclusion(identity_accessPlatformSources.iam_members,"owner","r.company_id","candidate.user_id"," IN(r.created_by,r.submitted_by,o.recorded_by)")}))`)} eligible_count
 FROM procurement_rfqs r LEFT JOIN procurement_rfq_offers o ON o.id=r.selected_offer_id WHERE ${scope} AND r.status NOT IN ('ordered','cancelled') ORDER BY r.needed_date`).bind(...args(a)).all<Row>()).results;
 },};return createRfqApplication(ports);
}
async function commit<Result>(write:SqlDatabase,a:WorkContext,m:RfqMutation<Evidence,Authority>,result:Result):Promise<Result>{
 const {key,operation,id,input,now}=m;
 let steps:SqlStatement[];let branchId:string,departmentId:string|null,version:number;
 if(m.kind==='create'){
 const {source,warehouse,config}=m;version=1;branchId=String(warehouse.branch_id);departmentId=source?.request.department_id??a.departmentId;
 steps=[nativeProcurementStatement('procurement_stmt_13', { now: Date.now() }).bind(id,a.companyId,warehouse.branch_id,departmentId,m.input.warehouseId,config.currency,m.input.title,m.input.demandEvidence,m.input.neededDate,a.userId,now,now,source?.request.id??null,source?.request.version??null,source?.request.evidence_version??null,source?.request.items_json??null),assertion(write,key,'warehouse',pointVersionGuard(`(${commercePlatformSources.commerce_warehouses}) commerce_warehouses`,` AND status='active'`),[warehouse.id,a.companyId,warehouse.version]),...(m.authority??[])];
 for(const d of m.demands)steps.push(assertion(write,key,`product-${d.ordinal}`,pointVersionGuard(`(${commercePlatformSources.commerce_products}) commerce_products`,"",` AND status='active'`),[d.product.id,a.companyId,d.product.version]),nativeProcurementStatement('procurement_stmt_14', { now: Date.now() }).bind(`${id}-${d.ordinal}`,id,d.product.id,d.product.name,d.product.unit,d.quantityMilli,d.ordinal,d.mapping?.ordinal??null,d.mapping?.requestMilli??null,d.mapping?.requestUnit??null,d.mapping?.factorMilli??null));
 steps.push(event(write,a,id,key,1,'create',m.input.demandEvidence,null,now),await nativeRfqNotification(write,a,id,key));
 }else{
 const r=m.rfq;version=m.input.expectedVersion+1;branchId=String(r.branch_id);departmentId=r.department_id?String(r.department_id):null;
 steps=[guard(write,a,r,key,m.input.expectedVersion)];
 if(r.request_id&&m.input.action!=='cancel')steps.push(assertion(write,key,'rfq-request-approval',approvedRequestExists(`(${recordsPlatformSources.employee_requests}) employee_requests`,'',`id=? AND company_id=? AND kind='purchase' AND state='in_progress'`,'?','?'),[r.request_id,a.companyId,r.request_evidence_version,r.request_items_json]));
 if(m.kind==='offer'){
 const offer=`rfq-offer-${key}`,supplier=m.offer.supplier;
 steps.push(...(m.evidenceBinding?.guards??[]),assertion(write,key,'supplier',pointVersionGuard(`(${financePlatformSources.finance_parties}) finance_parties`,"",` AND status='active' AND currency=?`),[supplier.id,a.companyId,supplier.version,r.currency]),nativeProcurementStatement('procurement_stmt_15', { now: Date.now() }).bind(offer,id,m.input.supplierId,m.input.expectedVersion,m.input.reference,m.input.evidence,m.input.arrivalDate,m.input.validUntil,r.currency,m.offer.subtotal,m.offer.tax,a.userId,now));
 for(const l of m.offer.lines)steps.push(nativeProcurementStatement('procurement_stmt_16', { now: Date.now() }).bind(`${offer}-${l.i}`,offer,l.l.lineId,l.price,l.l.taxBps,l.subtotal,l.tax));
 steps.push(...(m.evidenceBinding?.steps??[]));
 }else if(m.input.action==='submit')steps.push(nativeProcurementStatement('procurement_stmt_17', { now: Date.now() }).bind(m.offerId,m.input.reason,a.userId,id));
 else if(m.input.action==='approve')steps.push(nativeProcurementStatement('procurement_stmt_18', { now: Date.now() }).bind(a.userId,id));
 else if(m.input.action==='return')steps.push(nativeProcurementStatement('procurement_stmt_19', { now: Date.now() }).bind(id));
 steps.push(nativeProcurementStatement('procurement_stmt_20', { now: Date.now() }).bind(m.status,now,id),event(write,a,id,key,version,m.input.action,m.evidence,m.offerId,now),await nativeRfqNotification(write,a,id,key));
 }
 try{return await executeCommand(write,a,key,operation,input,id,version,result,steps,{companyId:a.companyId,branchId,departmentId});}catch(error){if(m.kind==='create'&&String(error).includes('RFQ_REQUEST_OVERALLOCATED'))throw new ApiError(409,'RFQ_REQUEST_OVERALLOCATED','Another quotation request allocated this demand. Refresh and use only the remaining approved quantity.');throw error;}
}
async function invoke<T>(call:()=>Promise<T>):Promise<T>{try{return await call();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
async function readPurchase(a:WorkContext){const database=(await holdingResources(a.holdingId)).write;return requireProcurementPurchasedRead(database,a);}
const purchaseControls=(access:Awaited<ReturnType<typeof procurementPurchaseAccess>>)=>access?{newWork:access.newWork,settleExisting:access.settleExisting,history:access.history,revision:access.revision}:undefined;
export const getRfq=(a:WorkContext,id:string)=>invoke(async()=>{await readPurchase(a);return application(a).get(id);});
export const rfqWorkspace=(a:WorkContext)=>invoke(async()=>{const access=await readPurchase(a),data=await application(a).workspace();return {...data,canPrepare:data.canPrepare&&(access?.newWork??true),purchase:purchaseControls(access)};});
export const rfqDetail=(a:WorkContext,id:string)=>invoke(async()=>{const access=await readPurchase(a),data=await application(a).detail(id);return {...data,canPrepare:data.canPrepare&&(access?.newWork??true),canApprove:data.canApprove&&(access?.newWork??true),canCancel:data.canPrepare&&(access?.settleExisting??true)&&["collecting","review","approved"].includes(String(data.rfq.status))&&(data.rfq.status!=="approved"||hasPermission(a,"commerce.approve")&&!samePerson(a,data.rfq.created_by)),purchase:purchaseControls(access)};});
export const createRfq=(a:WorkContext,raw:unknown,key:string)=>invoke(()=>withProcurementPurchasedCommand(a,{key,operation:"procurement.rfq.create",target:{kind:"rfq",id:`rfq-${key}`},creating:true,purpose:"new-work",permissions:["commerce.purchase"]},database=>application(a,database).create(raw,key)));
export const actRfq=(a:WorkContext,id:string,raw:unknown,key:string)=>invoke(()=>{const input=rfqActionSchema.parse(raw);return withProcurementPurchasedCommand(a,{key,operation:`procurement.rfq.${input.action}`,target:{kind:"rfq",id},purpose:input.action==="cancel"?"settle-existing":"new-work",permissions:["commerce.read","commerce.purchase",...(["approve","return"].includes(input.action)?["commerce.approve"]:[])]},database=>application(a,database).act(id,raw,key));});
export const prepareRfqOrderHandoff=(db:SqlDatabase,a:WorkContext,input:RfqOrderInput,orderId:string,key:string,editing:boolean)=>invoke(()=>application(a,db).handoff(input,orderId,key,editing));
export const rfqWork=(a:WorkContext)=>invoke(async()=>{if(!hasPermission(a,"commerce.read"))return [];const database=(await holdingResources(a.holdingId)).write,access=await procurementPurchaseAccess(database,a);if(access&&!access.history)return [];const items=await application(a).work();return access&&!access.newWork?items.map(item=>({...item,canAct:false,queue:"waiting" as const})):items;});
