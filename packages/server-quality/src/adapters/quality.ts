import {qualitySourceAccess,qualityVisibleHolds,requireQualityPurchasedRead,withQualityPurchasedCommand} from "@erpx/server-quality/adapters/purchases";
import {qualityCancellationSchema} from "@erpx/server-quality/domain/quality";
import { commercialOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as inventoryProjectionSources } from "@erpx/server-inventory/public/commercial-owner-projection-sources";
import {qualityInspectionFiles} from '@erpx/server-files/public/quality-evidence';
import {qualityScrapStockFacts} from '@erpx/server-commerce/public/quality-scrap-stock';
import {qualityScrapLotFacts} from '@erpx/server-inventory/public/quality-scrap-lots';
import {prepareQualityNotification as notify,prepareQualityScrapNotification} from '@erpx/server-collaboration/public/quality-notifications';
import {prepareQualityScrapCount,submitQualityScrapCount} from '@erpx/server-inventory/public/quality-scrap';
import {BusinessRuleError} from "@erpx/server-shared/index";
import 'server-only';
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {samePerson} from "@erpx/feature-identity_access/lib/iam";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {ApiError} from "@erpx/server-runtime/public/http";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {sha256} from "@erpx/server-runtime/public/crypto";
import {prepareBusinessEvidence} from "@erpx/server-records/public/business-evidence";

import type {SqlStatement} from '@erpx/server-runtime/public/runtime/types';
import {createQualityApplication,type QualityRepository,type QualityMutation} from '@erpx/server-quality/application/quality';
import type {QualityRow as Row,QualityInspectionInput,QualityReleaseInput,QualityDispositionInput,ReceiptQualityHold} from '@erpx/server-quality/domain/quality';
export {qualityInspectionSchema,qualityCancellationSchema,qualityReleaseSchema,qualityDispositionSchema} from '@erpx/server-quality/domain/quality';
const eligible=`a.company_id=h.company_id AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=h.branch_id)) AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,CASE WHEN h.status IN ('awaiting_release','cancel_pending') THEN 'quality.release' ELSE 'quality.inspect' END) AND NOT iam_same_person(a.user_id,h.created_by,h.company_id) AND (h.status<>'cancel_pending' OR NOT iam_same_person(a.user_id,h.cancel_requested_by,h.company_id)) AND (h.status<>'awaiting_release' OR NOT iam_same_person(a.user_id,h.last_inspector,h.company_id))`;

async function rows(db:SqlDatabase,actor:WorkContext,id?:string){return qualityVisibleHolds(db,actor,(await db.prepare(`SELECT h.*,p.name product_name,p.unit product_unit,p.tracking_mode,w.name warehouse_name,l.lot_code,l.serial_number,l.expiry_date,(SELECT count(DISTINCT a.user_id) FROM (${identity_accessProjectionSources.assignments}) a WHERE ${eligible}) eligible_count FROM inventory_quality_holds h JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=h.product_id JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=h.warehouse_id LEFT JOIN (${inventoryProjectionSources.inventory_lots}) l ON l.id=h.lot_id WHERE h.company_id=? AND (?::text IS NULL OR h.branch_id=?) AND (?::text IS NULL OR h.id=?) ORDER BY h.due_date,h.id`).bind(actor.companyId,actor.branchId,actor.branchId,id??null,id??null).all<Row>()).results);}

function application(actor:WorkContext,provided?:SqlDatabase){let resource:Promise<SqlDatabase>|null=provided?Promise.resolve(provided):null;return createQualityApplication({departmentScope:Boolean(actor.departmentId),companyScope:!actor.branchId,userId:actor.userId,authorize:permission=>requirePermission(actor,permission),can:permission=>hasPermission(actor,permission),samePerson:id=>samePerson(actor,id),clock:{now:()=>Date.now()},hash:sha256,repository:async()=>repository(await (resource??=holdingResources(actor.holdingId).then(r=>r.write)),actor)});}
function repository(write:SqlDatabase,actor:WorkContext):QualityRepository<SqlStatement[]>{return {
 holds:id=>rows(write,actor,id),
 replay:(key,operation,id,input)=>replayCommand(write,actor,key,operation,id,input),
 activeSpec:productId=>write.prepare("SELECT id,version FROM inventory_quality_specs WHERE company_id=? AND product_id=? AND status='active'").bind(actor.companyId,productId).first<Row>(),
 metrics:specId=>write.prepare('SELECT * FROM inventory_quality_spec_metrics WHERE spec_id=? ORDER BY ordinal').bind(specId).all<Row>().then(r=>r.results),inspectionFiles:(id,attachmentIds)=>qualityInspectionFiles(write,actor,id,attachmentIds).then(r=>r.results),scrapStock:h=>qualityScrapStockFacts(write,actor,h),scrapLots:h=>qualityScrapLotFacts(write,actor,h).then(r=>r.results),
 prepareEvidence:input=>prepareBusinessEvidence(write,actor,input),
 evidenceVersionGuard:(id,expectedVersion,key)=>[assertion(write,key,'quality-evidence-version',"EXISTS(SELECT 1 FROM inventory_quality_holds WHERE id=? AND company_id=? AND version=? AND status IN ('held','awaiting_release')",[id,actor.companyId,expectedVersion])],
 receiptHold:(input,spec,now)=>{const steps=[];if(spec)steps.push(assertion(write,input.key,'quality-spec',"EXISTS(SELECT 1 FROM inventory_quality_specs WHERE id=? AND version=? AND status='active')",[spec.id,spec.version]));steps.push(write.prepare('INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,created_by,created_at,updated_at,lot_id,held_quantity_milli,source_type,source_id,spec_id,spec_version,quantity_scope) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,\'partial\')').bind(input.id,actor.companyId,input.branchId,input.warehouseId,input.productId,input.reason,input.dueDate,actor.userId,now,now,input.lotId??null,input.quantityMilli,input.sourceType??'goods_receipt',input.sourceId,spec?.id??null,spec?.version??null),notify(write,input.key,input.id));return steps;
},commit:mutation=>commit(write,actor,mutation),
};}
async function commit(write:SqlDatabase,actor:WorkContext,mutation:QualityMutation<SqlStatement[]>){const {key,id,operation,now}=mutation;switch(mutation.action){
case 'inspect':{const {input,h,inspectionId,result,status,measurements,attachmentBinding}=mutation;const steps=[...(attachmentBinding?.guards??[]),assertion(write,key,'version',"EXISTS(SELECT 1 FROM inventory_quality_holds WHERE id=? AND company_id=? AND version=? AND status IN ('held','awaiting_release'))",[id,actor.companyId,input.expectedVersion])];
 if(h.spec_id)steps.push(assertion(write,key,'spec-snapshot',"EXISTS(SELECT 1 FROM inventory_quality_specs WHERE id=? AND version=? AND status IN ('active','superseded'))",[h.spec_id,h.spec_version]));
 steps.push(write.prepare('INSERT INTO inventory_quality_inspections(id,hold_id,reviewed_version,result,evidence,inspected_by,created_at,spec_id,spec_version) VALUES(?,?,?,?,?,?,?,?,?)').bind(inspectionId,id,input.expectedVersion,result,input.evidence,actor.userId,now,h.spec_id??null,h.spec_version??null));
 input.attachmentIds.forEach((attachmentId,index)=>steps.push(write.prepare('INSERT INTO inventory_quality_inspection_attachments(inspection_id,attachment_id,ordinal) VALUES(?,?,?)').bind(inspectionId,attachmentId,index)));
 measurements.forEach(({metric,value,passed},index)=>{steps.push(write.prepare('INSERT INTO inventory_quality_measurements(id,inspection_id,metric_id,code,label,unit,minimum_value,maximum_value,measured_value,passed,ordinal) VALUES(?,?,?,?,?,?,?,?,?,?,?)').bind(`${inspectionId}-metric-${index}`,inspectionId,metric.id,metric.code,metric.label,metric.unit,metric.minimum_value,metric.maximum_value,value,passed?1:0,metric.ordinal));});
 if(attachmentBinding)steps.push(...attachmentBinding.steps);
 steps.push(write.prepare('UPDATE inventory_quality_holds SET status=?,last_result=?,last_inspector=?,version=version+1,updated_at=? WHERE id=?').bind(status,result,actor.userId,now,id),notify(write,key,id));
 return executeCommand(write,actor,key,operation,input,id,input.expectedVersion+1,{id,status,result,version:input.expectedVersion+1},steps,{companyId:actor.companyId,branchId:String(h.branch_id),departmentId:null});
}
case 'release':{const {input,h}=mutation;return executeCommand(write,actor,key,operation,input,id,input.expectedVersion+1,{id,status:'released',version:input.expectedVersion+1},[assertion(write,key,'version',"EXISTS(SELECT 1 FROM inventory_quality_holds WHERE id=? AND company_id=? AND version=? AND status='awaiting_release' AND last_result='pass')",[id,actor.companyId,input.expectedVersion]),write.prepare("UPDATE inventory_quality_holds SET status='released',released_by=?,release_reason=?,released_at=?,version=version+1,updated_at=? WHERE id=?").bind(actor.userId,input.reason,now,now,id),notify(write,key,id)],{companyId:actor.companyId,branchId:String(h.branch_id),departmentId:null});}
case 'scrap':{const {input,h,stock,quantity,countId,attachmentBinding}=mutation;const steps=[
  ...(attachmentBinding?.guards??[]),
  assertion(write,key,'hold-version',"EXISTS(SELECT 1 FROM inventory_quality_holds WHERE id=? AND company_id=? AND version=? AND status='held' AND last_result='fail')",[id,actor.companyId,input.expectedVersion]),
  assertion(write,key,'stock-version',`EXISTS(SELECT 1 FROM (${inventoryProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=? AND version=? AND quantity_milli=? AND value_minor=?)`,[h.warehouse_id,h.product_id,stock.version,stock.quantity_milli,stock.value_minor]),
  assertion(write,key,'movement-date',`NOT EXISTS(SELECT 1 FROM (${inventoryProjectionSources.inventory_ledger}) inventory_ledger WHERE warehouse_id=? AND product_id=? AND movement_date>?)`,[h.warehouse_id,h.product_id,input.dispositionDate]),
  ...prepareQualityScrapCount(write,actor,mutation),
 ];
 
 if(attachmentBinding)steps.push(...attachmentBinding.steps);
 steps.push(
  submitQualityScrapCount(write,countId),
  write.prepare("UPDATE inventory_quality_holds SET status='disposition_pending',disposition_kind='scrap',disposition_reason=?,disposition_requested_by=?,disposition_count_id=?,disposition_quantity_milli=?,version=version+1,updated_at=? WHERE id=?").bind(input.reason,actor.userId,countId,quantity,now,id),
  write.prepare("INSERT INTO inventory_quality_disposition_events(id,hold_id,count_id,action,actor_id,reviewed_version,reason,created_at) VALUES(?,?,?,'request',?,?,?,?)").bind(`quality-disposition-${key}`,id,countId,actor.userId,input.expectedVersion,input.reason,now),
  prepareQualityScrapNotification(write,actor,key,countId,now)
 );
 return executeCommand(write,actor,key,operation,input,id,input.expectedVersion+1,{id,status:'disposition_pending',version:input.expectedVersion+1,countId},steps,{companyId:actor.companyId,branchId:String(h.branch_id),departmentId:null});
}
case 'cancel':{const {input,h,request,status}=mutation;const steps=[assertion(write,key,'uninspected-hold',"EXISTS(SELECT 1 FROM inventory_quality_holds WHERE id=? AND company_id=? AND version=? AND status=? AND last_result IS NULL AND NOT EXISTS(SELECT 1 FROM inventory_quality_inspections WHERE hold_id=?))",[id,actor.companyId,input.expectedVersion,request?'held':'cancel_pending',id])];
 if(request)steps.push(write.prepare("UPDATE inventory_quality_holds SET status='cancel_pending',cancel_requested_by=?,cancel_reason=?,version=version+1,updated_at=? WHERE id=?").bind(actor.userId,input.reason,now,id));
 else if(input.action==='approve')steps.push(write.prepare("UPDATE inventory_quality_holds SET status='cancelled',cancelled_by=?,cancelled_at=?,version=version+1,updated_at=? WHERE id=?").bind(actor.userId,now,now,id));
 else steps.push(write.prepare("UPDATE inventory_quality_holds SET status='held',version=version+1,updated_at=? WHERE id=?").bind(now,id));
 steps.push(write.prepare('INSERT INTO inventory_quality_cancellations VALUES(?,?,?,?,?,?,?)').bind(`quality-cancel-${key}`,id,input.expectedVersion+1,input.action,input.reason,actor.userId,now),notify(write,key,id));
 return executeCommand(write,actor,key,operation,input,id,input.expectedVersion+1,{id,status,version:input.expectedVersion+1},steps,{companyId:actor.companyId,branchId:String(h.branch_id),departmentId:null});
}
}}
async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export async function getQualityHold(actor:WorkContext,id:string,provided?:SqlDatabase){const write=provided??(await holdingResources(actor.holdingId)).write;await requireQualityPurchasedRead(write,actor,id);return translated(()=>application(actor,write).getQualityHold(id));}
export async function qualityEvidenceGuard(db:SqlDatabase,actor:WorkContext,id:string,expectedVersion:number,key:string){await requireQualityPurchasedRead(db,actor,id);return translated(()=>application(actor,db).qualityEvidenceGuard(id,expectedVersion,key));}
/** Original protective receiving/return effects compose under the full named
 * Commerce owner. Shared quarantine facts never acquire a management gate. */
export function receiptQualityHoldSteps(db:SqlDatabase,actor:WorkContext,input:ReceiptQualityHold){return translated(()=>application(actor,db).receiptQualityHoldSteps(input));}
export async function inspectQualityHold(actor:WorkContext,id:string,input:QualityInspectionInput,key:string){const {write}=await holdingResources(actor.holdingId);return withQualityPurchasedCommand(write,actor,{key,operation:"quality.hold.inspect",target:{kind:"hold",id},purpose:"settle-existing",permissions:["quality.inspect"],existing:true},db=>translated(()=>application(actor,db).inspectQualityHold(id,input,key)));}
export async function releaseQualityHold(actor:WorkContext,id:string,input:QualityReleaseInput,key:string){const {write}=await holdingResources(actor.holdingId);return withQualityPurchasedCommand(write,actor,{key,operation:"quality.hold.release",target:{kind:"hold",id},purpose:"settle-existing",permissions:["quality.release"],existing:true},db=>translated(()=>application(actor,db).releaseQualityHold(id,input,key)));}
export async function createQualityScrapDisposition(actor:WorkContext,id:string,input:QualityDispositionInput,key:string){const {write}=await holdingResources(actor.holdingId);return withQualityPurchasedCommand(write,actor,{key,operation:"quality.hold.disposition.scrap",target:{kind:"hold",id},purpose:"settle-existing",permissions:["quality.release"],existing:true,disposition:true},db=>translated(()=>application(actor,db).createQualityScrapDisposition(id,input,key)));}
export async function qualityWork(actor:WorkContext){if(!hasPermission(actor,"quality.read")||actor.departmentId)return [];const {write}=await holdingResources(actor.holdingId),items=await translated(()=>application(actor,write).qualityWork());return Promise.all(items.map(async item=>{const access=await qualitySourceAccess(write,actor,item.id);return !access||access.settleExisting?item:{...item,canAct:false,actionKind:"work" as const,queue:item.queue==="completed"?"completed" as const:"waiting" as const,relationships:(item.relationships??[]).filter(value=>!["assigned","approval"].includes(value))};}));}
export async function cancelQualityHold(actor:WorkContext,id:string,raw:unknown,key:string){const {write}=await holdingResources(actor.holdingId),input=qualityCancellationSchema.parse(raw);return withQualityPurchasedCommand(write,actor,{key,operation:`quality.hold.cancel.${input.action}`,target:{kind:"hold",id},purpose:"settle-existing",permissions:[input.action==="request"?"quality.hold":"quality.release"],existing:true},db=>translated(()=>application(actor,db).cancelQualityHold(id,input,key)));}
