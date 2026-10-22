import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {WorkItem} from "@erpx/feature-work_projection/lib/work-item";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {hasModuleAccess} from "@erpx/catalogue/module-access";
import {currentAssignmentReadScope} from "@erpx/server-identity_access/public/record-source-authority";
import {servicePackageEnforcementMode,readPurchasedAccess,checkPurchasedCapability} from "@erpx/server-commercial/public/purchase-command-boundary";
import {packageClockSql,purchasedCapabilityPredicate} from "@erpx/server-commercial/public/purchased-capability-sql";
import {scheduledFeaturePredicate} from "@erpx/server-pms/public/hospitality-rollout";
import {fieldServiceCustomerSourceRows} from "@erpx/server-finance/public/field-service-references";
import {fieldServiceAssetSourceRows} from "@erpx/server-assets/public/field-service-references";
import {fieldServiceTicketSourceRows} from "@erpx/server-support/public/field-service-sources";
import {ApiError} from "@erpx/server-runtime/public/http";

const nativeIds="SELECT id FROM field_service_agreements UNION SELECT id FROM field_service_orders";
/** Actual owner references, without private snapshots or a caller source label.
 * A visit retains its original ticket/customer/agreement and optional asset. */
const nativeSources=`SELECT agreement.id,agreement.company_id,agreement.branch_id,agreement.department_id,'agreement' kind,(agreement.asset_id IS NOT NULL) asset
 FROM field_service_agreements agreement JOIN (${fieldServiceCustomerSourceRows}) customer ON customer.id=agreement.customer_id AND customer.company_id=agreement.company_id AND customer.kind='customer'
 LEFT JOIN (${fieldServiceAssetSourceRows}) asset ON asset.id=agreement.asset_id AND asset.company_id=agreement.company_id AND asset.branch_id=agreement.branch_id
 WHERE agreement.asset_id IS NULL OR asset.id IS NOT NULL
 UNION ALL SELECT visit.id,visit.company_id,visit.branch_id,visit.department_id,'order',(visit.asset_id IS NOT NULL OR agreement.asset_id IS NOT NULL)
 FROM field_service_orders visit JOIN field_service_agreements agreement ON agreement.id=visit.agreement_id AND agreement.company_id=visit.company_id AND agreement.branch_id=visit.branch_id AND agreement.department_id IS NOT DISTINCT FROM visit.department_id
 JOIN (${fieldServiceCustomerSourceRows}) customer ON customer.id=agreement.customer_id AND customer.company_id=visit.company_id AND customer.kind='customer'
 JOIN (${fieldServiceTicketSourceRows}) ticket ON ticket.id=visit.ticket_id AND ticket.company_id=visit.company_id AND ticket.customer_id=agreement.customer_id AND ticket.branch_id=visit.branch_id AND ticket.department_id IS NOT DISTINCT FROM visit.department_id
 LEFT JOIN (${fieldServiceAssetSourceRows}) asset ON asset.id=visit.asset_id AND asset.company_id=visit.company_id AND asset.branch_id=visit.branch_id
 WHERE (agreement.asset_id IS NULL OR visit.asset_id=agreement.asset_id) AND (visit.asset_id IS NULL OR asset.id IS NOT NULL)`;
const selectedModule="(jsonb_array_length(selected.modules_json::jsonb)=0 OR selected.modules_json::jsonb @> '[\"field_service\"]'::jsonb)";
function authority(actor:WorkContext){
 const current=currentAssignmentReadScope(actor,"workforce_actor_scope"),permitted=hasModuleAccess(actor.modules,"field_service")&&hasPermission(actor,"records.read");
 return {where:`${permitted?'TRUE':'FALSE'} AND ${current.where} AND iam_role_allows(?,?,'records.read') AND EXISTS(SELECT 1 FROM assignments selected WHERE selected.id=? AND ${selectedModule})`,args:[...current.args,actor.role,actor.companyId,actor.assignmentId]};
}
function visible(actor:WorkContext,id:string,company:string,purchased=true,orderOnly=false){
 const current=authority(actor);
 return {where:`EXISTS(WITH service_source_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms) SELECT 1 FROM (${nativeSources}) service_source CROSS JOIN service_source_clock CROSS JOIN LATERAL(SELECT service_source.company_id,service_source.branch_id,service_source.department_id)workforce_actor_scope
 WHERE service_source.id=${id} AND service_source.company_id=${company} AND ${orderOnly?"service_source.kind='order'":"TRUE"} AND (?::text IS NULL OR service_source.branch_id=?) AND (?::text IS NULL OR service_source.department_id=?) AND ${current.where}
 AND ${purchased?purchasedCapabilityPredicate('service_source.company_id','service.orders','history','service_source_clock.now_ms'):'TRUE'}
 AND (NOT service_source.asset OR ${purchased?purchasedCapabilityPredicate('service_source.company_id','service.parts-assets','history','service_source_clock.now_ms'):'TRUE'}) AND ${scheduledFeaturePredicate('service_source.company_id','field_service')})`,args:[actor.branchId,actor.branchId,actor.departmentId,actor.departmentId,...current.args]};
}
export async function serviceSourceReadScope(database:SqlDatabase,actor:WorkContext,surface:"records"|"staff"|"inbox"){
 const mode=await servicePackageEnforcementMode();if(mode!=="enforced"){if(mode==="shadow")await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:"service.orders"});return {where:"TRUE",args:[] as (string|number|null)[]};}
 const id=surface==="records"?"workspace_records.id":surface==="staff"?"staff_notifications.record_id":"e.source_id",company=surface==="records"?"workspace_records.company_id":surface==="staff"?"staff_notifications.company_id":"e.company_id",type=surface==="staff"?"staff_notifications.record_type":"e.source_type",source=visible(actor,id,company,true,surface!=="records");
 if(surface!=="records")return {where:`(${type}<>'field_service_order' OR ${source.where})`,args:source.args};
 const current=authority(actor);
 return {where:`(NOT EXISTS(SELECT 1 FROM (${nativeIds}) service_actual WHERE service_actual.id=${id}) OR ${source.where}) AND (module_id<>'field_service' OR EXISTS(SELECT 1 FROM(SELECT ${company} company_id,workspace_records.branch_id,workspace_records.department_id,${packageClockSql} now_ms)workforce_actor_scope WHERE ${current.where} AND ${purchasedCapabilityPredicate('workforce_actor_scope.company_id','service.orders','history','workforce_actor_scope.now_ms')} AND ${scheduledFeaturePredicate('workforce_actor_scope.company_id','field_service')}))`,args:[...source.args,...current.args]};
}
export async function requireServiceWorkspaceRecordRead(database:SqlDatabase,actor:WorkContext,moduleId:string,id:string){
 const mode=await servicePackageEnforcementMode();if(mode==="disabled")return;
 const actual=await database.prepare(`SELECT 1 FROM (${nativeIds}) service_actual WHERE id=?`).bind(id).first();if(moduleId!=="field_service"&&!actual)return;
 if(mode==="shadow"){await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:"service.orders"});return;}
 await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:"service.orders",purpose:"history"});
 const source=actual?visible(actor,"service_input.id","service_input.company_id"):null,current=authority(actor);
 const where=source?.where??`EXISTS(SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ${current.where})`,args=source?.args??[actor.companyId,actor.branchId,actor.departmentId,...current.args];
 if(!await database.prepare(`SELECT ${where} allowed FROM(SELECT ?::text id,?::text company_id)service_input`).bind(...args,id,actor.companyId).first("allowed"))throw new ApiError(404,"FIELD_SERVICE_SOURCE_NOT_FOUND","The actual Service source is not available to this current assignment.");
}
/** Native source-record consumers use the same current source in their
 * versioned guard, including purchase changes after the initial read. */
export async function serviceRecordSourcePredicate(database:SqlDatabase,actor:WorkContext,alias:"s"|"o"){
 const mode=await servicePackageEnforcementMode();if(mode!=="enforced"){if(mode==="shadow")await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:"service.orders"});return {where:"TRUE",args:[] as (string|number|null)[]};}
 return visible(actor,`${alias}.id`,`${alias}.company_id`,true,true);
}
export async function filterServiceWork(database:SqlDatabase,actor:WorkContext,items:WorkItem[]){
 if(await servicePackageEnforcementMode()==="disabled"||!items.some(item=>item.type==="field_service"))return items;
 let current:WorkItem[];try{current=await(await import('@erpx/server-field_service/public/field-service')).fieldServiceWork(actor,database);}catch(error){if(error instanceof ApiError&&[403,404].includes(error.status))return items.filter(item=>item.type!=="field_service");throw error;}
 const source=visible(actor,"service_work.id","service_work.company_id"),ids=items.filter(item=>item.type==="field_service").map(item=>item.id),allowed=await servicePackageEnforcementMode()==="enforced"?new Set((await database.prepare(`SELECT service_work.id FROM (${nativeSources}) service_work WHERE service_work.company_id=? AND ${source.where} AND service_work.id IN (${ids.map(()=>'?').join(',')})`).bind(actor.companyId,...source.args,...ids).all<{id:string}>()).results.map(row=>row.id)):null;
 const sources=new Map(current.filter(item=>!allowed||allowed.has(item.id)).map(item=>[item.id,item]));return items.flatMap(item=>{if(item.type!=="field_service")return [item];const source=sources.get(item.id);if(!source)return [];const canAct=item.canAct&&source.canAct;return [{...item,canAct,...(!canAct?{queue:item.queue==='completed'?'completed' as const:'waiting' as const,actionKind:'work' as const,relationships:(item.relationships??[]).filter(value=>!['assigned','approval'].includes(value)),flow:item.flow?{...item.flow,canAct:false,canManage:false}:undefined}:{})}];});
}
export async function serviceMetricHistoryAccess(database:SqlDatabase,actor:WorkContext){
 const mode=await servicePackageEnforcementMode();let allowed=true;
 if(mode!=="disabled"){
  const purchase=await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:"service.orders"});
  if(mode==="enforced"){const current=authority(actor);allowed=Boolean(purchase.history&&await database.prepare(`SELECT ${current.where} allowed FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope`).bind(...current.args,actor.companyId,actor.branchId,actor.departmentId).first("allowed"));}
 }
 return {"open-service-orders":allowed,"service-billing-releases":allowed,"field-visit-completion":allowed};
}

/** Evidence detail uses the exact source predicate from its aggregate and
 * bounded search before reading the normal retained owner detail. */
export async function serviceEvidenceDetail(database:SqlDatabase,actor:WorkContext,id:string){
 const source=await serviceRecordSourcePredicate(database,actor,"o");
 if(!await database.prepare(`SELECT o.id FROM field_service_orders o WHERE o.id=? AND o.company_id=? AND ${source.where}`).bind(id,actor.companyId,...source.args).first())throw new ApiError(404,"FIELD_SERVICE_SOURCE_NOT_FOUND","The Service evidence source is not available in this current role.");
 return (await import('@erpx/server-field_service/public/field-service')).fieldServiceDetail(actor,id,database);
}
