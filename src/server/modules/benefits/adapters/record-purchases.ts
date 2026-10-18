import "server-only";
import type {WorkContext} from "../../identity_access/public/authorization";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import type {WorkItem} from "@/lib/work_projection/work-item";
import {hasPermission} from "@/lib/identity_access/permissions";
import {hasModuleAccess} from "@/data/module-access";
import {currentAssignmentReadScope} from "../../identity_access/public/record-source-authority";
import {peoplePackageEnforcementMode,checkPurchasedCapability,readPurchasedAccess} from "../../commercial/public/purchase-command-boundary";
import {packageClockSql,purchasedCapabilityPredicate} from "../../commercial/public/purchased-capability-sql";
import {scheduledFeaturePredicate} from "../../pms/public/hospitality-rollout";
import {benefitAttachmentPeopleSources as people} from "../../people_payroll/public/benefits-owner-facts";
import {ApiError} from "../../runtime/public/http";
import {benefitPurchaseControls,benefitAssignmentModulePredicate} from "./purchases";
const nativeSources=`SELECT r.id,r.company_id,'plan' kind,NULL::text person_user_id,TRUE current_person FROM benefit_plans r
 UNION ALL SELECT r.id,r.company_id,'window',NULL::text,TRUE FROM benefit_enrollment_windows r
 UNION ALL SELECT r.id,r.company_id,'dependent',p.user_id,p.status='active' AND p.merged_into_id IS NULL AND EXISTS(SELECT 1 FROM (${people.engagements}) e WHERE e.person_id=p.id AND e.company_id=p.company_id AND e.status='active') FROM benefit_dependents r JOIN (${people.people}) p ON p.id=r.person_id AND p.company_id=r.company_id
 UNION ALL SELECT r.id,r.company_id,'election',p.user_id,p.status='active' AND p.merged_into_id IS NULL AND EXISTS(SELECT 1 FROM (${people.engagements}) e WHERE e.person_id=p.id AND e.company_id=p.company_id AND e.status='active') FROM benefit_elections r JOIN benefit_plans plan ON plan.id=r.plan_id AND plan.company_id=r.company_id JOIN (${people.people}) p ON p.id=r.person_id AND p.company_id=r.company_id`;
function visible(actor:WorkContext,id:string,company:string,purchased=true,electionOnly=false){
 const current=currentAssignmentReadScope(actor,"workforce_actor_scope"),read=hasModuleAccess(actor.modules,"benefits")&&hasPermission(actor,"records.read"),review=hasPermission(actor,"benefits.review")?"iam_role_allows(?,?,'benefits.review')":"FALSE",reviewArgs=hasPermission(actor,"benefits.review")?[actor.role,actor.companyId]:[];
 return {sql:`EXISTS(WITH benefit_source_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms) SELECT 1 FROM (${nativeSources}) benefit_source CROSS JOIN benefit_source_clock CROSS JOIN LATERAL(SELECT benefit_source.company_id,?::text branch_id,?::text department_id)workforce_actor_scope
 WHERE benefit_source.id=${id} AND benefit_source.company_id=${company} AND ${electionOnly?"benefit_source.kind='election'":"TRUE"} AND ${read?'TRUE':'FALSE'} AND ${current.where} AND iam_role_allows(?,?,'records.read') AND EXISTS(SELECT 1 FROM assignments selected WHERE selected.id=? AND ${benefitAssignmentModulePredicate})
 AND (benefit_source.kind IN ('plan','window') OR (benefit_source.person_user_id=? AND benefit_source.current_person) OR (benefit_source.kind='election' AND ${review}))
 AND ${purchased?purchasedCapabilityPredicate('benefit_source.company_id','people.benefits','history','benefit_source_clock.now_ms'):'TRUE'} AND ${scheduledFeaturePredicate('benefit_source.company_id','benefits')})`,args:[actor.branchId,actor.departmentId,...current.args,actor.role,actor.companyId,actor.assignmentId,actor.userId,...reviewArgs]};
}
/** Product-owned generic Benefits records require the addon. Actual private
 * elections/dependents additionally retain the owner's employee/reviewer policy. */
export async function benefitSourceReadScope(database:SqlDatabase,actor:WorkContext,surface:"records"|"staff"|"inbox"){
 const mode=await peoplePackageEnforcementMode();if(mode!=="enforced"){if(mode==="shadow")await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:"people.benefits"});return {where:"TRUE",args:[] as (string|number|null)[]};}
 const id=surface==="records"?"workspace_records.id":surface==="staff"?"staff_notifications.record_id":"e.source_id",company=surface==="records"?"workspace_records.company_id":surface==="staff"?"staff_notifications.company_id":"e.company_id",type=surface==="staff"?"staff_notifications.record_type":"e.source_type",source=visible(actor,id,company,true,surface!=="records");
 if(surface!=="records")return {where:`(${type}<>'benefit_election' OR ${source.sql})`,args:source.args};
 // Unknown legacy generic rows retain their original record policy, but are
 // not a bypass around purchasing their actual Benefits module.
 return {where:`(NOT EXISTS(SELECT 1 FROM (${nativeSources}) benefit_actual WHERE benefit_actual.id=${id}) OR ${source.sql}) AND (module_id<>'benefits' OR EXISTS(SELECT 1 FROM(SELECT ${packageClockSql} now_ms)benefit_generic_clock WHERE ${purchasedCapabilityPredicate(company,'people.benefits','history','benefit_generic_clock.now_ms')}))`,args:source.args};
}
export async function requireBenefitWorkspaceRecordRead(database:SqlDatabase,actor:WorkContext,moduleId:string,id:string){
 const mode=await peoplePackageEnforcementMode();if(mode==="disabled")return;
 const actual=await database.prepare(`SELECT 1 FROM (${nativeSources}) benefit_actual WHERE id=?`).bind(id).first();if(moduleId!=="benefits"&&!actual)return;
 if(mode==="shadow"){await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:"people.benefits"});return;}
 await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:"people.benefits",purpose:"history"});
 if(actual){const source=visible(actor,"benefit_input.id","benefit_input.company_id",false);if(!await database.prepare(`SELECT ${source.sql} allowed FROM(SELECT ?::text id,?::text company_id)benefit_input`).bind(...source.args,id,actor.companyId).first("allowed"))throw new ApiError(404,"BENEFIT_SOURCE_NOT_FOUND","The actual Benefits source is unavailable in this current role.");}
}
/** Repeat after generic Flow enrichment so stale private sources or action
 * flags cannot be reintroduced into a later shared Work presentation. */
export async function filterBenefitWork(_database:SqlDatabase,actor:WorkContext,items:WorkItem[]){
 if(await peoplePackageEnforcementMode()==="disabled"||!items.some(item=>item.type==="benefit_enrollment"))return items;
 let current:WorkItem[];try{current=await(await import('../public/benefits')).benefitWork(actor);}catch(error){if(error instanceof ApiError&&[403,404].includes(error.status))return items.filter(item=>item.type!=="benefit_enrollment");throw error;}
 const sources=new Map(current.map(item=>[item.id,item]));return items.flatMap(item=>{if(item.type!=="benefit_enrollment")return [item];const source=sources.get(item.id);if(!source)return [];const canAct=item.canAct&&source.canAct;return [{...item,canAct,...(!canAct?{queue:item.queue==='completed'?'completed' as const:'waiting' as const,actionKind:'work' as const,relationships:(item.relationships??[]).filter(value=>!['assigned','approval'].includes(value)),flow:item.flow?{...item.flow,canAct:false,canManage:false}:undefined}:{})}];});
}
export async function benefitMetricHistoryAccess(database:SqlDatabase,actor:WorkContext){
 const mode=await peoplePackageEnforcementMode();if(mode!=="enforced"){if(mode==="shadow")await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:"people.benefits"});return {"active-benefit-elections":true,"published-benefit-plans":true};}
 let allowed=false;try{allowed=Boolean((await benefitPurchaseControls(actor,database))?.history);}catch(error){if(!(error instanceof ApiError&&[403,404].includes(error.status)))throw error;}
 return {"active-benefit-elections":allowed,"published-benefit-plans":allowed};
}

export async function benefitMetricSourceScope(actor:WorkContext){
 if(await peoplePackageEnforcementMode()!=="enforced")return {where:"TRUE",args:[] as (string|number|null)[]};
 const current=currentAssignmentReadScope(actor,"workforce_actor_scope"),permitted=hasModuleAccess(actor.modules,"benefits")&&hasPermission(actor,"records.read");
 return {where:`EXISTS(SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id,${packageClockSql} now_ms)workforce_actor_scope WHERE ${permitted?'TRUE':'FALSE'} AND ${current.where} AND iam_role_allows(?,?,'records.read') AND EXISTS(SELECT 1 FROM assignments selected WHERE selected.id=? AND ${benefitAssignmentModulePredicate}) AND ${purchasedCapabilityPredicate('workforce_actor_scope.company_id','people.benefits','history','workforce_actor_scope.now_ms')} AND ${scheduledFeaturePredicate('workforce_actor_scope.company_id','benefits')})`,args:[actor.companyId,actor.branchId,actor.departmentId,...current.args,actor.role,actor.companyId,actor.assignmentId]};
}

/** Aggregate counts are the original coarse Company facts. Drilldown and
 * evidence additionally retain the private actual-election reader policy. */
export async function benefitMetricRecordSourceScope(actor:WorkContext){if(await peoplePackageEnforcementMode()!=="enforced")return null;const source=visible(actor,"e.id","e.company_id",true,true);return {where:source.sql,args:source.args};}

export async function benefitEvidenceSourceScope(actor:WorkContext,metric:"active-benefit-elections"|"published-benefit-plans"){const source=visible(actor,metric==="active-benefit-elections"?"e.id":"p.id",metric==="active-benefit-elections"?"e.company_id":"p.company_id",await peoplePackageEnforcementMode()==="enforced",metric==="active-benefit-elections");return {where:source.sql,args:source.args};}
