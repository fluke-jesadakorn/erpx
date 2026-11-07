import 'server-only';

import { createHash } from 'node:crypto';
import type {SqlDatabase,SqlValue} from '@erpx/server-runtime/public/runtime/types';

import {ApiError} from '@erpx/server-runtime/public/http';
import type {StaffActor} from '@erpx/server-operations/public/staff-policy';
import { workOperationsSettlementSources } from '@erpx/server-operations/public/work-package-obligation-sources';
import {projectTaskRecordPolicy} from '@erpx/server-operations/public/project-task-record-policy';
import { projectSocialPurchaseSources, projectSocialSourceReadPolicy } from '@erpx/server-projects/public/social-purchase-sources';
import { socialProjectAudienceReadPredicate } from '@erpx/server-projects/public/collaboration-home-sources';
import { socialPmsSourceFacts } from '@erpx/server-pms/public/social-source-facts';
import { pmsSocialSourcePurchasePredicate } from '@erpx/server-pms/public/social-source-purchases';

import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import {currentAssignmentReadScope} from '@erpx/server-identity_access/public/record-source-authority';

import { appsPackageEnforcementMode, projectsPackageEnforcementMode, pmsPackageEnforcementMode, readPurchasedAccesses } from '@erpx/server-commercial/public/purchase-command-boundary';
import type {CapabilityPurpose} from '@erpx/server-commercial/public/rules/packages';
import {purchasedCapabilityPredicate,packageClockSql} from '@erpx/server-commercial/public/purchased-capability-sql';
import {workSocialNativePostPredicate,workSocialUnresolvedPostPredicate,operationalHandoverReceiptPredicate,operationalHandoverBindings} from '@erpx/server-collaboration/adapters/work-social-source';

import {sourceAccessSql} from '@erpx/server-collaboration/public/social-source-policy';

import{units}from '@erpx/server-collaboration/adapters/work-social-unit-state';
type Reader=Pick<StaffActor,'companyId'|'userId'>&Partial<StaffActor>;
type ReadScope={prefix:string;where:string;args:SqlValue[]};
export const actorKey=(actor:StaffActor)=>JSON.stringify([actor.companyId,actor.branchId,actor.departmentId,actor.userId,actor.assignmentId,actor.role,actor.authority??null,actor.iam??null]);
export const clock=packageClockSql;
export function selected(actor:Reader):actor is StaffActor{return typeof actor.assignmentId==='string'&&typeof actor.role==='string'&&'branchId' in actor&&'departmentId' in actor;}
export function actorScope(actor:Reader){
 if(selected(actor)){const current=currentAssignmentReadScope(actor,'workforce_actor_scope');return {prefix:`WITH social_purchase_actor AS MATERIALIZED(SELECT ?::text user_id,?::text assignment_id,?::text role,?::text company_id,${clock} now_ms WHERE EXISTS(SELECT 1 FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) workforce_actor_scope WHERE ${current.where})) `,args:[actor.userId,actor.assignmentId,actor.role,actor.companyId,actor.companyId,actor.branchId,actor.departmentId,...current.args] as SqlValue[]};}
 // Recipient visibility probes keep their original existential current account
 // policy. No replacement acting role or queued authority is selected here.
 return {prefix:`WITH social_purchase_actor AS MATERIALIZED(SELECT g.user_id,g.id assignment_id,g.role,g.company_id,${clock} now_ms FROM assignments g WHERE g.user_id=? AND g.company_id=? AND (?::text IS NULL OR g.id=?) AND iam_assignment_active(g.id,${clock}) AND iam_principal_active(g.user_id,g.company_id,${clock})) `,args:[actor.userId,actor.companyId,actor.assignmentId??null,actor.assignmentId??null] as SqlValue[]};
}
export const sourceRows=`SELECT ps.source_kind kind,ps.source_id id FROM social_post_sources ps WHERE ps.post_id=p.id`;
export function projectRead(purpose:CapabilityPurpose,enforced:boolean){return `NOT EXISTS(SELECT 1 FROM (${sourceRows}) source JOIN (${projectSocialPurchaseSources}) social_project_source ON social_project_source.source_kind=source.kind AND social_project_source.source_id=source.id AND social_project_source.company_id=p.company_id WHERE NOT (${projectSocialSourceReadPolicy()} AND ${enforced?`${scheduledFeaturePredicate('p.company_id','projects')} AND (source.kind<>'staff_task' OR ${scheduledFeaturePredicate('p.company_id','operations')}) AND iam_role_allows(social_purchase_actor.role,p.company_id,'records.read') AND ${purchasedCapabilityPredicate('p.company_id','projects.wbs',purpose,'social_purchase_actor.now_ms')} AND (source.kind<>'staff_task' OR ${purchasedCapabilityPredicate('p.company_id','projects.tasks',purpose,'social_purchase_actor.now_ms')})`:'TRUE'})) AND ${enforced?socialProjectAudienceReadPredicate():'TRUE'}`;}
export function operationalRead(purpose:CapabilityPurpose){return `NOT EXISTS(SELECT 1 FROM (${sourceRows}) source JOIN (${workOperationsSettlementSources.tasks}) t ON source.kind='staff_task' AND t.id=source.id AND t.company_id=p.company_id WHERE t.project_id IS NULL AND NOT (${projectTaskRecordPolicy('t.id','t.company_id','social_purchase_actor')} AND ${purchasedCapabilityPredicate('t.company_id','work.actions',purpose,'social_purchase_actor.now_ms')} AND (t.template_id IS NULL OR ${purchasedCapabilityPredicate('t.company_id','work.advanced-routines',purpose,'social_purchase_actor.now_ms')})))`;}
export function purchasedReadWhere(project:boolean,stories:boolean,typed=operationalHandoverReceiptPredicate('p'),purpose:CapabilityPurpose='history'){
 const purchased=purchasedCapabilityPredicate(stories?'social_stories.company_id':'p.company_id','work.social',purpose,'social_purchase_actor.now_ms');
 return `EXISTS(SELECT 1 FROM social_purchase_actor WHERE ${scheduledFeaturePredicate('social_purchase_actor.company_id','collaboration')} AND ${stories?purchased:`CASE WHEN ${typed} THEN ${operationalRead(purpose)} AND ${projectRead(purpose,project)} ELSE (${purchased} OR NOT (${workSocialNativePostPredicate('p')}) AND NOT (${workSocialUnresolvedPostPredicate()}) AND ${projectRead(purpose,project)}) END`})`;
}
export const projectOnlyReadWhere=()=>`EXISTS(SELECT 1 FROM social_purchase_actor WHERE ${projectRead('history',true)})`;
export function pmsRead(purpose:CapabilityPurpose){return `EXISTS(SELECT 1 FROM social_purchase_actor WHERE NOT EXISTS(SELECT 1 FROM (${sourceRows}) source WHERE source.kind='pms_record' AND NOT EXISTS(SELECT 1 FROM (${socialPmsSourceFacts}) social_pms_source WHERE social_pms_source.id=source.id AND social_pms_source.company_id=p.company_id AND ${pmsSocialSourcePurchasePredicate(purpose)})))`;}
export async function socialOwnerModes(){const [work,project,pms]=await Promise.all([appsPackageEnforcementMode(),projectsPackageEnforcementMode(),pmsPackageEnforcementMode()]);return {work,project,pms,disabled:work==='disabled'&&project==='disabled'&&pms==='disabled'};}
export function workSocialReadScope(database:SqlDatabase,actor:Reader,stories=false):Promise<ReadScope>{return socialReadScope(database,actor,stories);}
export function workSocialProductReadScope(database:SqlDatabase,actor:Reader):Promise<ReadScope>{return socialReadScope(database,actor,false,true);}
export async function socialReadScope(database:SqlDatabase,actor:Reader,stories=false,strict=false):Promise<ReadScope>{
 const {work:mode,project,pms}=strict?{work:'enforced' as const,project:'enforced' as const,pms:'enforced' as const}:await socialOwnerModes();if(mode==='shadow')await readPurchasedAccesses(database,mode,actor.companyId,['work.social']);if(mode!=='enforced'){if(stories||project!=='enforced'&&pms!=='enforced')return {prefix:'',where:'TRUE',args:[]};const scope=actorScope(actor);return {...scope,where:[project==='enforced'?projectOnlyReadWhere():'TRUE',pms==='enforced'?pmsRead('history'):'TRUE'].join(' AND ')};}
 const scope=actorScope(actor),handover=units.get(database)?.handover;
 let typed=operationalHandoverReceiptPredicate('p');if(handover){scope.prefix=scope.prefix.trimEnd()+`,social_operational_handover AS MATERIALIZED(SELECT ?::text post_id,?::jsonb sources) `;scope.args.push(handover.id,JSON.stringify(handover.sources));typed=`(${typed} OR EXISTS(SELECT 1 FROM social_operational_handover temporary WHERE p.id=temporary.post_id AND ${operationalHandoverBindings('p')}=temporary.sources))`;}
 // Paid Social keeps the original audience/linked-preview policy. Included
 // Project/PMS text additionally needs its actual source owner; it cannot
 // become free general Social merely by naming a business record.
 return {...scope,where:`(${purchasedReadWhere(project==='enforced',stories,typed)}) AND ${!stories&&project==='enforced'?`EXISTS(SELECT 1 FROM social_purchase_actor WHERE ${projectRead('history',true)})`:'TRUE'} AND ${!stories&&pms==='enforced'?pmsRead('history'):'TRUE'}`};
}
export async function workSocialPresentation(database:SqlDatabase,actor:StaffActor,ids:readonly string[]=[]){
 const {work:mode,project,pms}=await socialOwnerModes();
 const context=createHash('sha256').update(actorKey(actor)).digest('hex');
 if(mode!=='enforced'&&project!=='enforced'&&pms!=='enforced')return {context,purchase:undefined,posts:undefined};
 const accesses=await readPurchasedAccesses(database,mode,actor.companyId,['work.social','work.actions']);
 const actualRevision=mode==='disabled'?(await readPurchasedAccesses(database,'enforced',actor.companyId,['pms.front-desk']))['pms.front-desk'].revision:accesses['work.social'].revision;
 const scope=actorScope(actor),revision=createHash('sha256').update(JSON.stringify([actualRevision,context])).digest('hex');
 const base=mode==='enforced'?`EXISTS(SELECT 1 FROM social_purchase_actor WHERE ${scheduledFeaturePredicate('social_purchase_actor.company_id','collaboration')})`:'TRUE';
 const taskReady=Boolean(await database.prepare(`${scope.prefix}SELECT 1 FROM social_purchase_actor WHERE ${scheduledFeaturePredicate('social_purchase_actor.company_id','operations')}`).bind(...scope.args).first());
 const active=Boolean(await database.prepare(`${scope.prefix}SELECT 1 WHERE ${base}`).bind(...scope.args).first());
 const minimal=(value:typeof accesses[string])=>({newWork:active&&value.newWork,settleExisting:active&&value.settleExisting,history:active&&value.history,revision});
 const purchase=mode==='enforced'?minimal(accesses['work.social']):undefined;
 if(!ids.length)return {context,purchase,posts:undefined};
 const ownerDecision=(purpose:CapabilityPurpose)=>mode==='enforced'?`(${purchasedReadWhere(project==='enforced',false,operationalHandoverReceiptPredicate('p'),purpose)}) AND EXISTS(SELECT 1 FROM social_purchase_actor WHERE NOT (${operationalHandoverReceiptPredicate('p')}) OR ${project!=='enforced'?'TRUE':`NOT EXISTS(SELECT 1 FROM (${sourceRows}) source JOIN (${projectSocialPurchaseSources}) social_project_source ON social_project_source.source_kind=source.kind AND social_project_source.source_id=source.id AND social_project_source.company_id=p.company_id WHERE NOT (${purchasedCapabilityPredicate('p.company_id','projects.handover',purpose,'social_purchase_actor.now_ms')}))`})`:project==='enforced'?`EXISTS(SELECT 1 FROM social_purchase_actor WHERE ${projectRead(purpose,true)})`:'TRUE';
 const decision=(purpose:CapabilityPurpose)=>`(${ownerDecision(purpose)}) AND ${project==='enforced'?`EXISTS(SELECT 1 FROM social_purchase_actor WHERE ${projectRead(purpose,true)})`:'TRUE'} AND ${pms==='enforced'?pmsRead(purpose):'TRUE'}`;
 const rows=(await database.prepare(`${scope.prefix}SELECT p.id,(${decision('new-work')}) new_work,(${decision('settle-existing')}) settle_existing,(${decision('history')}) history FROM social_posts p WHERE p.id IN(SELECT jsonb_array_elements_text(?::jsonb)) AND p.company_id=? AND p.state='published' AND ${sourceAccessSql('p','?::text')}`).bind(...scope.args,JSON.stringify(ids),actor.companyId,actor.userId,Date.now(),Date.now(),actor.assignmentId).all<{id:string;new_work:boolean;settle_existing:boolean;history:boolean}>()).results;
 return {context,purchase,posts:Object.fromEntries(rows.map(row=>[row.id,{newWork:row.new_work,settleExisting:row.settle_existing,history:row.history,revision,createTask:row.new_work&&taskReady&&accesses['work.actions'].newWork}]))};
}
export async function requireWorkSocialRead(database:SqlDatabase,actor:Reader,id:string){
 const scope=await workSocialReadScope(database,actor);if(!scope.prefix)return;
 if(!await database.prepare(`${scope.prefix}SELECT 1 FROM social_posts p WHERE p.id=? AND p.company_id=? AND ${scope.where}`).bind(...scope.args,id,actor.companyId).first())throw new ApiError(404,'POST_NOT_FOUND','This post is unavailable.');
}
