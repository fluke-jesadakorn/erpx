import { platformOwnerProjectionSources as commercePlatformSources } from "../../commerce/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as financePlatformSources } from "../../finance/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as identity_accessPlatformSources } from "../../identity_access/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as recordsPlatformSources } from "../../records/public/platform-owner-projection-sources";
import "server-only";
import type {WorkContext} from "../../identity_access/public/authorization";
import type {SqlDatabase,SqlStatement} from "../../runtime/public/runtime/types";
import {samePerson} from "@/lib/identity_access/iam";
import {hasPermission} from "@/lib/identity_access/permissions";
import {requirePermission} from "../../identity_access/public/iam-permissions";
import {holdingResources} from "../../runtime/public/runtime";
import {ApiError} from "../../runtime/public/http";
import {assertion,executeCommand,replayCommand} from "../../workflow/public/commands";
import {requestQuotationSource,requestVersionGuard} from "../../records/public/employee-requests";
import {serviceAuthoritySnapshot} from "../../support/public/service-access";
import {prepareAttachmentBinding} from "../../files/public/attachments/service";
import {stageRfqNotifications} from "../../collaboration/public/rfq-notifications";
import {BusinessRuleError} from "../../shared";
import {createRfqApplication,type RfqPorts,type RfqMutation} from "../application/rfqs";
import type {RfqRow as Row,RfqOrderInput} from "../domain/rfqs";
import {rfqActionSchema} from "../domain/rfqs";
import {procurementPurchaseAccess,requireProcurementPurchasedRead,withProcurementPurchasedCommand,prepareRfqPurchaseHandoff} from "./purchases";
export {rfqCreateSchema,rfqActionSchema} from "../domain/rfqs";
type Evidence=Awaited<ReturnType<typeof prepareAttachmentBinding>>;
type Authority=SqlStatement[];
const scope="r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?)";
const args=(a:WorkContext)=>[a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId];
function guard(db:SqlDatabase,a:WorkContext,r:Row,key:string,version:number){return assertion(db,key,'rfq-version',`EXISTS(SELECT 1 FROM procurement_rfqs r WHERE r.id=? AND r.version=? AND r.status=? AND ${scope})`,[r.id,version,r.status,...args(a)]);}
function event(db:SqlDatabase,a:WorkContext,id:string,key:string,version:number,action:string,evidence:string,offer:string|null,now:number){return db.prepare('INSERT INTO procurement_rfq_events(id,rfq_id,version,action,actor_id,evidence,selected_offer_id,created_at) VALUES(?,?,?,?,?,?,?,?)').bind(`rfq-event-${key}`,id,version,action,a.userId,evidence,offer,now);}
function application(a:WorkContext,provided?:SqlDatabase){
 let resource:Promise<SqlDatabase>|null=null;const database=()=>provided?Promise.resolve(provided):resource??=holdingResources(a.holdingId).then(x=>x.write);
 const ports:RfqPorts<Evidence,Authority,SqlStatement>={clock:{now:()=>Date.now()},canRead:hasPermission(a,'commerce.read'),canPurchase:hasPermission(a,'commerce.purchase'),canApprove:hasPermission(a,'commerce.approve'),samePerson:(id)=>samePerson(a,id),
 async authorizeRead(){requirePermission(a,'commerce.read');},async authorizePurchase(){requirePermission(a,'commerce.purchase');},async authorizeApprove(){requirePermission(a,'commerce.approve');},
 replay:async(key,operation,id,input)=>replayCommand(await database(),a,key,operation,id,input),
 rfq:async(id)=>(await database()).prepare(`SELECT r.* FROM procurement_rfqs r WHERE r.id=? AND ${scope}`).bind(id,...args(a)).first<Row>(),
 detail:async(id)=>{const write=await database();const [lines,offers,offerLines,events]=await Promise.all([
 write.prepare('SELECT * FROM procurement_rfq_lines WHERE rfq_id=? ORDER BY ordinal').bind(id).all<Row>(),
 write.prepare(`SELECT o.*,p.name supplier_name FROM procurement_rfq_offers o JOIN (${financePlatformSources.finance_parties}) p ON p.id=o.supplier_id WHERE o.rfq_id=? ORDER BY o.rfq_version DESC`).bind(id).all<Row>(),
 write.prepare('SELECT l.* FROM procurement_rfq_offer_lines l JOIN procurement_rfq_offers o ON o.id=l.offer_id WHERE o.rfq_id=?').bind(id).all<Row>(),
 write.prepare('SELECT * FROM procurement_rfq_events WHERE rfq_id=? ORDER BY version DESC').bind(id).all<Row>()]);return {lines:lines.results,offers:offers.results,offerLines:offerLines.results,events:events.results};},
 workspace:async()=>{const write=await database();const [rfqs,products,warehouses,suppliers,config]=await Promise.all([
 write.prepare(`SELECT r.* FROM procurement_rfqs r WHERE ${scope} ORDER BY r.updated_at DESC LIMIT 100`).bind(...args(a)).all<Row>(),
 write.prepare(`SELECT id,sku,name,unit FROM (${commercePlatformSources.commerce_products}) commerce_products WHERE company_id=? AND status='active' ORDER BY sku`).bind(a.companyId).all<Row>(),
 write.prepare(`SELECT id,name,branch_id FROM (${commercePlatformSources.commerce_warehouses}) commerce_warehouses WHERE company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?) ORDER BY name`).bind(a.companyId,a.branchId,a.branchId).all<Row>(),
 write.prepare(`SELECT id,name FROM (${financePlatformSources.finance_parties}) finance_parties WHERE company_id=? AND kind='supplier' AND status='active' ORDER BY name`).bind(a.companyId).all<Row>(),
 write.prepare(`SELECT currency FROM (${commercePlatformSources.commerce_settings}) commerce_settings WHERE company_id=?`).bind(a.companyId).first<Row>()]);return {rfqs:rfqs.results,products:products.results,warehouses:warehouses.results,suppliers:suppliers.results,currency:String(config?.currency??'')};},
 sourceState:async(id)=>(await database()).prepare(`SELECT state,evidence_version,items_json,approval_json FROM (${recordsPlatformSources.employee_requests}) employee_requests WHERE id=? AND company_id=?`).bind(id,a.companyId).first<Row>(),
 quotationSource:async(id)=>requestQuotationSource(await database(),a,id),currentQuotationSource:async(id)=>{try{return await requestQuotationSource(await database(),a,id);}catch(error){if(error instanceof ApiError&&[403,404,409].includes(error.status))return null;throw error;}},
 configuration:async()=>(await database()).prepare(`SELECT currency FROM (${commercePlatformSources.commerce_settings}) commerce_settings WHERE company_id=?`).bind(a.companyId).first<Row>(),
 warehouse:async(id)=>(await database()).prepare(`SELECT * FROM (${commercePlatformSources.commerce_warehouses}) commerce_warehouses WHERE id=? AND company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?)`).bind(id,a.companyId,a.branchId,a.branchId).first<Row>(),
 product:async(id)=>(await database()).prepare(`SELECT * FROM (${commercePlatformSources.commerce_products}) commerce_products WHERE id=? AND company_id=? AND status='active'`).bind(id,a.companyId).first<Row>(),
 supplier:async(id,currency)=>(await database()).prepare(`SELECT * FROM (${financePlatformSources.finance_parties}) finance_parties WHERE id=? AND company_id=? AND kind='supplier' AND status='active' AND currency=?`).bind(id,a.companyId,currency).first<Row>(),
 prepareSource:async(source,key)=>{const write=await database(),snapshot=await serviceAuthoritySnapshot(write,a.companyId);return [...snapshot(key),requestVersionGuard(write,key,source.request)];},
 prepareEvidence:async(id,offer,input,key)=>prepareAttachmentBinding(await database(),a,{attachmentSetIds:input.attachmentSetIds,bindingContract:{targetType:'procurement_rfq',bindingType:'procurement_rfq_offer',contexts:[{purposeKey:'supplier_quotation',componentKind:'action',componentRef:'supplier-offer'}]},targetId:id,expectedTargetVersion:input.expectedVersion,commandId:key,bindingId:offer}),
 commit:async(m,result)=>commit(await database(),a,m,result),
 linkedRfq:async(id)=>(await database()).prepare('SELECT id FROM procurement_rfqs WHERE order_id=? AND company_id=?').bind(id,a.companyId).first<Row>(),
 stageHandoff:async(h)=>{const db=await database(),r=h.detail.rfq;
 await prepareRfqPurchaseHandoff(db,a,String(r.id),h.orderId,h.key,h.editing);
 const sourceGuard=r.request_id?[assertion(db,h.key,'rfq-source-evidence',`EXISTS(SELECT 1 FROM (${recordsPlatformSources.employee_requests}) e JOIN procurement_request_orders l ON l.request_id=e.id AND l.order_id=? AND l.rfq_id=? WHERE e.id=? AND e.company_id=? AND e.evidence_version=? AND e.items_json=? AND e.approval_json::jsonb->>'decision'='approved')`,[h.orderId,r.id,r.request_id,a.companyId,r.request_evidence_version,r.request_items_json])]:[];
 const currencyGuard=assertion(db,h.key,'rfq-currency',`EXISTS(SELECT 1 FROM (${commercePlatformSources.commerce_settings}) commerce_settings WHERE company_id=? AND currency=?)`,[a.companyId,r.currency]);
 if(h.editing)return [currencyGuard,...sourceGuard,guard(db,a,r,h.key,Number(r.version))];
 return [currencyGuard,...sourceGuard,guard(db,a,r,h.key,h.order.rfqVersion??0),db.prepare("UPDATE procurement_rfqs SET status='ordered',order_id=?,version=version+1,updated_at=? WHERE id=?").bind(h.orderId,h.now,r.id),event(db,a,String(r.id),h.key,Number(r.version)+1,'order-created',`Draft purchase order ${h.orderId} created; normal PO approval still required.`,String(h.selected.id),h.now),stageRfqNotifications(db,String(r.id),h.key)];},
 workRows:async()=>{const write=await database();
return (await write.prepare(`SELECT r.*,o.recorded_by offer_recorder,CASE WHEN r.request_id IS NULL THEN 1 WHEN EXISTS(SELECT 1 FROM (${recordsPlatformSources.employee_requests}) source WHERE source.id=r.request_id AND source.company_id=r.company_id AND source.state='in_progress' AND source.evidence_version=r.request_evidence_version AND source.items_json=r.request_items_json AND source.approval_json::jsonb->>'decision'='approved') THEN 1 ELSE 0 END source_ready,
 (SELECT count(DISTINCT candidate.user_id) FROM (${identity_accessPlatformSources.assignments}) candidate WHERE candidate.company_id=r.company_id AND iam_assignment_active(candidate.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(candidate.role,candidate.company_id,'commerce.purchase') AND (candidate.scope_kind='holding' OR (candidate.scope_kind='branch' AND candidate.scope_node_id=r.branch_id) OR (candidate.scope_kind='department' AND candidate.scope_node_id=r.department_id)) AND (r.status<>'review' OR (iam_role_allows(candidate.role,candidate.company_id,'commerce.approve') AND candidate.user_id<>r.created_by AND candidate.user_id<>COALESCE(r.submitted_by,'') AND candidate.user_id<>COALESCE(o.recorded_by,'') AND NOT EXISTS(SELECT 1 FROM (${identity_accessPlatformSources.iam_members}) cm JOIN (${identity_accessPlatformSources.iam_members}) owner ON owner.company_id=cm.company_id AND owner.person_id=cm.person_id WHERE cm.company_id=r.company_id AND cm.user_id=candidate.user_id AND owner.user_id IN(r.created_by,r.submitted_by,o.recorded_by))))) eligible_count
 FROM procurement_rfqs r LEFT JOIN procurement_rfq_offers o ON o.id=r.selected_offer_id WHERE ${scope} AND r.status NOT IN ('ordered','cancelled') ORDER BY r.needed_date`).bind(...args(a)).all<Row>()).results;
 },};return createRfqApplication(ports);
}
async function commit<Result>(write:SqlDatabase,a:WorkContext,m:RfqMutation<Evidence,Authority>,result:Result):Promise<Result>{
 const {key,operation,id,input,now}=m;
 let steps:SqlStatement[];let branchId:string,departmentId:string|null,version:number;
 if(m.kind==='create'){
 const {source,warehouse,config}=m;version=1;branchId=String(warehouse.branch_id);departmentId=source?.request.department_id??a.departmentId;
 steps=[write.prepare("INSERT INTO procurement_rfqs(id,company_id,branch_id,department_id,warehouse_id,currency,title,demand_evidence,needed_date,status,created_by,created_at,updated_at,request_id,request_version,request_evidence_version,request_items_json) VALUES(?,?,?,?,?,?,?,?,?,'collecting',?,?,?,?,?,?,?)").bind(id,a.companyId,warehouse.branch_id,departmentId,m.input.warehouseId,config.currency,m.input.title,m.input.demandEvidence,m.input.neededDate,a.userId,now,now,source?.request.id??null,source?.request.version??null,source?.request.evidence_version??null,source?.request.items_json??null),assertion(write,key,'warehouse',`EXISTS(SELECT 1 FROM (${commercePlatformSources.commerce_warehouses}) commerce_warehouses WHERE id=? AND company_id=? AND status='active' AND version=?)`,[warehouse.id,a.companyId,warehouse.version]),...(m.authority??[])];
 for(const d of m.demands)steps.push(assertion(write,key,`product-${d.ordinal}`,`EXISTS(SELECT 1 FROM (${commercePlatformSources.commerce_products}) commerce_products WHERE id=? AND company_id=? AND version=? AND status='active')`,[d.product.id,a.companyId,d.product.version]),write.prepare('INSERT INTO procurement_rfq_lines(id,rfq_id,product_id,description,unit,quantity_milli,ordinal,request_item_ordinal,request_quantity_milli,request_unit,request_units_per_product_unit_milli) VALUES(?,?,?,?,?,?,?,?,?,?,?)').bind(`${id}-${d.ordinal}`,id,d.product.id,d.product.name,d.product.unit,d.quantityMilli,d.ordinal,d.mapping?.ordinal??null,d.mapping?.requestMilli??null,d.mapping?.requestUnit??null,d.mapping?.factorMilli??null));
 steps.push(event(write,a,id,key,1,'create',m.input.demandEvidence,null,now),stageRfqNotifications(write,id,key));
 }else{
 const r=m.rfq;version=m.input.expectedVersion+1;branchId=String(r.branch_id);departmentId=r.department_id?String(r.department_id):null;
 steps=[guard(write,a,r,key,m.input.expectedVersion)];
 if(r.request_id&&m.input.action!=='cancel')steps.push(assertion(write,key,'rfq-request-approval',`EXISTS(SELECT 1 FROM (${recordsPlatformSources.employee_requests}) employee_requests WHERE id=? AND company_id=? AND kind='purchase' AND state='in_progress' AND evidence_version=? AND items_json=? AND approval_json::jsonb->>'decision'='approved')`,[r.request_id,a.companyId,r.request_evidence_version,r.request_items_json]));
 if(m.kind==='offer'){
 const offer=`rfq-offer-${key}`,supplier=m.offer.supplier;
 steps.push(...(m.evidenceBinding?.guards??[]),assertion(write,key,'supplier',`EXISTS(SELECT 1 FROM (${financePlatformSources.finance_parties}) finance_parties WHERE id=? AND company_id=? AND version=? AND status='active' AND currency=?)`,[supplier.id,a.companyId,supplier.version,r.currency]),write.prepare('INSERT INTO procurement_rfq_offers VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)').bind(offer,id,m.input.supplierId,m.input.expectedVersion,m.input.reference,m.input.evidence,m.input.arrivalDate,m.input.validUntil,r.currency,m.offer.subtotal,m.offer.tax,a.userId,now));
 for(const l of m.offer.lines)steps.push(write.prepare('INSERT INTO procurement_rfq_offer_lines VALUES(?,?,?,?,?,?,?)').bind(`${offer}-${l.i}`,offer,l.l.lineId,l.price,l.l.taxBps,l.subtotal,l.tax));
 steps.push(...(m.evidenceBinding?.steps??[]));
 }else if(m.input.action==='submit')steps.push(write.prepare('UPDATE procurement_rfqs SET selected_offer_id=?,selection_reason=?,submitted_by=?,approved_by=NULL WHERE id=?').bind(m.offerId,m.input.reason,a.userId,id));
 else if(m.input.action==='approve')steps.push(write.prepare('UPDATE procurement_rfqs SET approved_by=? WHERE id=?').bind(a.userId,id));
 else if(m.input.action==='return')steps.push(write.prepare('UPDATE procurement_rfqs SET selected_offer_id=NULL,selection_reason=NULL,submitted_by=NULL,approved_by=NULL WHERE id=?').bind(id));
 steps.push(write.prepare('UPDATE procurement_rfqs SET status=?,version=version+1,updated_at=? WHERE id=?').bind(m.status,now,id),event(write,a,id,key,version,m.input.action,m.evidence,m.offerId,now),stageRfqNotifications(write,id,key));
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
