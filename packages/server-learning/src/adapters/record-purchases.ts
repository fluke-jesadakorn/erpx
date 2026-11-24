import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {WorkItem} from "@erpx/feature-work_projection/lib/work-item";
import {hasModuleAccess} from "@erpx/catalogue/module-access";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {ApiError} from "@erpx/server-runtime/public/http";
import {learningPackageEnforcementMode,readPurchasedAccess} from "@erpx/server-commercial/public/purchase-command-boundary";
import {purchasedCapabilityPredicate} from "@erpx/server-commercial/public/purchased-capability-sql";
import {actorScopeExistsProbe,clockProbe,nativeSourceProbe} from "./learning-read-sql";
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
function visible(actor:WorkContext,id:string,company:string){
 const permitted=hasModuleAccess(actor.modules,'learning')&&hasPermission(actor,'records.read'),capabilities=['learn.courses','learn.assignments','learn.results','learn.certificates'];
 return {sql:`EXISTS(WITH learning_source_clock AS ${clockProbe} ${nativeSourceProbe('learning_source')} CROSS JOIN learning_source_clock WHERE learning_source.id=${id} AND learning_source.company_id=${company} AND ${permitted?'TRUE':'FALSE'} AND (?::text IS NULL OR learning_source.capability='learn.courses' OR learning_source.branch_id=?) AND (?::text IS NULL OR learning_source.capability='learn.courses' OR learning_source.department_id=?) AND ${actorScopeExistsProbe('learning_source.company_id,?::text branch_id,?::text department_id',' ')} AND (${capabilities.map(capability=>`(learning_source.capability='${capability}' AND ${purchasedCapabilityPredicate('learning_source.company_id',capability,'history','learning_source_clock.now_ms')})`).join(' OR ')}) AND ${scheduledFeaturePredicate('learning_source.company_id','learning')})`,args:[actor.branchId,actor.branchId,actor.departmentId,actor.departmentId,actor.branchId,actor.departmentId,actor.assignmentId,actor.userId,actor.role]};
}
/** Evaluation may cite only an actual currently readable Learning result. */

export async function learningSourceReadScope(database:SqlDatabase,actor:WorkContext,surface:'records'|'staff'|'inbox'){
 const mode=await learningPackageEnforcementMode();if(mode!=='enforced'){if(mode==='shadow')await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:'learn.assignments'});return {where:'TRUE',args:[] as unknown[]};}
 const id=surface==='records'?'workspace_records.id':surface==='staff'?'staff_notifications.record_id':'e.source_id',company=surface==='records'?'workspace_records.company_id':surface==='staff'?'staff_notifications.company_id':'e.company_id',kind=surface==='staff'?'staff_notifications.record_type':'e.source_type',relevant=surface==='records'?`EXISTS(${nativeSourceProbe('actual_learning')} WHERE actual_learning.id=${id})`:`${kind} IN ('learning_course','learning_assignment','learning_result','learning_certificate','learning_certificate_revocation','learning_eligibility_handoff')`,scope=visible(actor,id,company);
 let generic='';if(surface==='records'){const access=await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:'learn.assignments'});generic=` AND (module_id<>'learning' OR ${access.history?'TRUE':'FALSE'})`;}
 return {where:`(NOT (${relevant}) OR ${scope.sql})${generic}`,args:scope.args};
}
export async function requireLearningWorkspaceRecordRead(database:SqlDatabase,actor:WorkContext,moduleId:string,id:string){
 const mode=await learningPackageEnforcementMode();if(mode==='disabled')return;
 if(moduleId==='learning'){const access=await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:'learn.assignments'});if(!access.history)throw new ApiError(403,'PACKAGE_REQUIRED','Learning history is unavailable.');}
 if(!await database.prepare(`${nativeSourceProbe('native')} WHERE native.id=?`).bind(id).first())return;
 if(mode==='shadow'){await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:'learn.assignments'});return;}
 const scope=visible(actor,'learning_record.id','learning_record.company_id'),allowed=await database.prepare(`SELECT ${scope.sql} allowed FROM (SELECT ?::text id,?::text company_id) learning_record`).bind(...scope.args,id,actor.companyId).first('allowed');if(!allowed)throw new ApiError(403,'LEARNING_SOURCE_FORBIDDEN','This Learning source is unavailable in the current assignment.');
}
export async function filterLearningWork(database:SqlDatabase,actor:WorkContext,items:WorkItem[]){
 if(await learningPackageEnforcementMode()==='disabled'||!items.some(item=>item.type==='learning_assignment'))return items;
 let current:WorkItem[];try{current=await (await import('@erpx/server-learning/public/learning')).learningWork(actor);}catch(error){if(error instanceof ApiError&&[403,404].includes(error.status))return items.filter(item=>item.type!=='learning_assignment');throw error;}
 const sources=new Map(current.map(item=>[item.id,item]));return items.flatMap(item=>{if(item.type!=='learning_assignment')return [item];const source=sources.get(item.id);if(!source)return [];const canAct=item.canAct&&source.canAct;return [{...item,canAct,...(!canAct?{queue:item.queue==='completed'?'completed' as const:'waiting' as const,actionKind:'work' as const,relationships:(item.relationships??[]).filter(value=>!['assigned','approval'].includes(value)),flow:item.flow?{...item.flow,canAct:false,canManage:false}:undefined}:{})}];});
}
