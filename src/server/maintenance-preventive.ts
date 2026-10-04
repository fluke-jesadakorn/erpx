import 'server-only';
import {z} from 'zod';
import type {WorkContext} from './authorization';
import type {SqlDatabase,SqlStatement} from './runtime/types';
import {isTransactionalSqlDatabase} from './runtime/types';
import {holdingResources} from './runtime';
import {requirePermission} from './iam-permissions';
import {hasPermission} from '@/lib/permissions';
import {hasModuleAccess} from '@/data/module-access';
import {ApiError} from './http';
import {assertion,executeCommand,replayCommand} from './commands';
import {createMaintenanceWork} from './asset-maintenance';
import {addDays} from '@/lib/people-roster';
import {localDateTimeCandidates,localMinuteAt} from '@/lib/calendar-timeline';
import type {WorkItem} from '@/lib/work-item';

const version=z.number().int().positive(),reason=z.string().trim().min(3).max(2000);
const timeZone=z.string().trim().min(2).max(80).refine(value=>{try{new Intl.DateTimeFormat('en',{timeZone:value});return true;}catch{return false;}},'Choose an IANA time zone.');
export const preventivePlanSchema=z.object({assetId:z.string().min(1),assetVersion:version,title:z.string().trim().min(3).max(160),priority:z.enum(['normal','high','critical']),
 ownerId:z.string().min(1),intervalDays:z.number().int().min(1).max(3650),firstDueAt:z.number().int().positive().safe(),timeZone,leadDays:z.number().int().min(0).max(60).default(7),expectedVersion:version.optional(),reason});
export const preventiveActionSchema=z.object({action:z.enum(['activate','pause','cancel','generate']),expectedVersion:version,reason});
type Plan={id:string;company_id:string;branch_id:string;department_id:string|null;asset_id:string;title:string;priority:'normal'|'high'|'critical';owner_id:string;interval_days:number;time_zone:string;first_due_local:string;next_due_local:string;next_due_at:number;lead_days:number;status:string;version:number;created_by:string;edited_by:string;approved_by:string|null;updated_at:number;asset_code?:string;asset_name?:string};
const scope='p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?)';
const scopeArgs=(a:WorkContext)=>[a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId];
const eligible=(user:string)=>`EXISTS(SELECT 1 FROM assignments x JOIN hr_people h ON h.user_id=x.user_id AND h.company_id=x.company_id JOIN hr_engagements e ON e.person_id=h.id AND e.company_id=h.company_id
 WHERE x.company_id=p.company_id AND x.user_id=${user} AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
 AND iam_principal_active(x.user_id,x.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(x.role,x.company_id,'records.write')
 AND (x.scope_kind='holding' OR x.scope_kind='branch' AND x.scope_node_id=p.branch_id OR x.scope_kind='department' AND x.scope_node_id=p.department_id)
 AND h.status='active' AND h.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))`;
function access(a:WorkContext,write=false){requirePermission(a,write?'records.write':'records.read');if(!hasModuleAccess(a.modules,'maintenance'))throw new ApiError(403,'MODULE_FORBIDDEN','Your current role cannot access Maintenance.');}
function wallClock(instant:number,zone:string){const local=localMinuteAt(instant,zone);return `${local.date}T${local.time}`;}
function nextOccurrence(local:string,zone:string,days:number){
 const nextLocal=`${addDays(local.slice(0,10),days)}${local.slice(10)}`;
 // Keep the plan's wall-clock definition. A spring-forward gap moves only that
 // occurrence to the next valid minute; a fall-back fold selects its first instant.
 for(let minute=0;minute<=180;minute++){
  const resolved=minute?new Date(Date.parse(`${nextLocal}:00Z`)+minute*60000).toISOString().slice(0,16):nextLocal,candidates=localDateTimeCandidates(resolved,zone);
  if(candidates.length)return {local:nextLocal,at:candidates[0],resolvedLocal:resolved};
 }
 throw new ApiError(422,'MAINTENANCE_TIME_UNAVAILABLE','Choose a plan time with an available occurrence in this time zone.');
}
async function loadPlan(db:SqlDatabase,a:WorkContext,id:string){const plan=await db.prepare(`SELECT p.*,a.code asset_code,a.name asset_name FROM maintenance_preventive_plans p JOIN operational_assets a ON a.id=p.asset_id WHERE p.id=? AND ${scope}`).bind(id,...scopeArgs(a)).first<Plan>();if(!plan)throw new ApiError(404,'MAINTENANCE_PLAN_NOT_FOUND','Preventive plan not found in this scope.');return plan;}
function event(db:SqlDatabase,key:string,id:string,action:string,a:WorkContext,note:string,now:number){return db.prepare('INSERT INTO maintenance_preventive_events SELECT ?,id,version,?,?,?,to_jsonb(p),? FROM maintenance_preventive_plans p WHERE id=?').bind(`preventive-event-${key}`,action,a.userId,note,now,id);}
function notice(db:SqlDatabase,key:string,id:string,message:string,now:number){return db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,company_id,owner_id,'maintenance_plan',id,version,?,? FROM maintenance_preventive_plans WHERE id=?").bind(`preventive-notice-${key}`,message,now,id);}

export async function preventivePlans(a:WorkContext,providedWrite?:SqlDatabase){access(a);const db=providedWrite??(await holdingResources(a.holdingId)).write;return (await db.prepare(`SELECT p.*,a.code asset_code,a.name asset_name,CASE WHEN ${eligible('p.owner_id')} THEN 1 ELSE 0 END owner_eligible,
 (SELECT legal_name FROM hr_people WHERE company_id=p.company_id AND user_id=p.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name
 FROM maintenance_preventive_plans p JOIN operational_assets a ON a.id=p.asset_id WHERE ${scope} ORDER BY p.next_due_at,p.id LIMIT 300`).bind(...scopeArgs(a)).all<Plan&{owner_eligible:number;owner_name:string|null}>()).results;}
export async function preventivePlanDetail(a:WorkContext,id:string){access(a);const db=(await holdingResources(a.holdingId)).write,plan=await loadPlan(db,a,id);return {plan,actorId:a.userId,canWrite:hasPermission(a,'records.write'),canReview:hasPermission(a,'records.review'),
 assetVersion:Number(await db.prepare('SELECT version FROM operational_assets WHERE id=? AND company_id=?').bind(plan.asset_id,a.companyId).first('version')),
 people:(await db.prepare(`SELECT DISTINCT person.user_id id,person.legal_name name FROM maintenance_preventive_plans p JOIN hr_people person ON person.company_id=p.company_id WHERE p.id=? AND person.status='active' AND person.merged_into_id IS NULL AND ${eligible('person.user_id')} ORDER BY person.legal_name`).bind(id).all<{id:string;name:string}>()).results,
 history:(await db.prepare('SELECT * FROM maintenance_preventive_events WHERE plan_id=? ORDER BY version').bind(id).all<Record<string,unknown>>()).results,
 occurrences:(await db.prepare('SELECT o.*,w.status,w.version work_version FROM maintenance_preventive_occurrences o JOIN maintenance_work_orders w ON w.id=o.work_order_id WHERE o.plan_id=? ORDER BY o.due_at DESC').bind(id).all<Record<string,unknown>>()).results};}

export async function savePreventivePlan(a:WorkContext,id:string|null,raw:unknown,key:string){
 access(a,true);const input=preventivePlanSchema.parse(raw),db=(await holdingResources(a.holdingId)).write,recordId=id??`maintenance-plan-${key}`,op=id?'maintenance.plan.revise':'maintenance.plan.create',prior=await replayCommand(db,a,key,op,recordId,input);if(prior)return prior;
 const current=id?await loadPlan(db,a,id):null;if(current&&(!['draft','paused'].includes(current.status)||current.asset_id!==input.assetId))throw new ApiError(409,'MAINTENANCE_PLAN_PAUSE_REQUIRED','Pause the same asset plan before revising its definition.');
 if(current&&current.created_by!==a.userId&&!hasPermission(a,'records.review'))throw new ApiError(403,'MAINTENANCE_PLAN_REVIEW_REQUIRED','The plan requester or an eligible reviewer must revise the definition.');
 const local=wallClock(input.firstDueAt,input.timeZone),dueAt=localDateTimeCandidates(local,input.timeZone)[0],now=Date.now(),nextVersion=current?Number(input.expectedVersion)+1:1;
 if(current&&!input.expectedVersion)throw new ApiError(422,'VERSION_REQUIRED','Use the current plan version.');
 const steps:SqlStatement[]=[db.prepare('SELECT id FROM operational_assets WHERE id=? FOR UPDATE').bind(input.assetId),
 assertion(db,key,'asset-source',`EXISTS(SELECT 1 FROM operational_assets p WHERE p.id=? AND p.version=? AND p.lifecycle_status IN ('registered','in_service','maintenance') AND ${scope})`,[input.assetId,input.assetVersion,...scopeArgs(a)])];
 if(current)steps.push(db.prepare('SELECT id FROM maintenance_preventive_plans WHERE id=? FOR UPDATE').bind(recordId),assertion(db,key,'plan-version',`EXISTS(SELECT 1 FROM maintenance_preventive_plans p WHERE p.id=? AND p.version=? AND p.status IN ('draft','paused') AND ${scope})`,[recordId,input.expectedVersion!,...scopeArgs(a)]),
 assertion(db,key,'retained-occurrences','NOT EXISTS(SELECT 1 FROM maintenance_preventive_occurrences WHERE plan_id=? AND due_local>=?)',[recordId,local]),
 db.prepare('UPDATE maintenance_preventive_plans SET title=?,priority=?,owner_id=?,interval_days=?,time_zone=?,first_due_local=?,next_due_local=?,next_due_at=?,lead_days=?,edited_by=?,approved_by=NULL,approved_at=NULL,version=version+1,updated_at=? WHERE id=?').bind(input.title,input.priority,input.ownerId,input.intervalDays,input.timeZone,local,local,dueAt,input.leadDays,a.userId,now,recordId));
 else steps.push(db.prepare(`INSERT INTO maintenance_preventive_plans(id,company_id,branch_id,department_id,asset_id,title,priority,owner_id,interval_days,time_zone,first_due_local,next_due_local,next_due_at,lead_days,created_by,edited_by,created_at,updated_at)
 SELECT ?,company_id,branch_id,department_id,id,?,?,?,?,?,?,?,?,?,?,?,?,? FROM operational_assets WHERE id=?`).bind(recordId,input.title,input.priority,input.ownerId,input.intervalDays,input.timeZone,local,local,dueAt,input.leadDays,a.userId,a.userId,now,now,input.assetId));
 steps.push(assertion(db,key,'eligible-owner',`EXISTS(SELECT 1 FROM maintenance_preventive_plans p WHERE p.id=? AND ${eligible('p.owner_id')})`,[recordId]),event(db,key,recordId,current?'revise':'create',a,input.reason,now),notice(db,key,recordId,'Review the preventive maintenance plan and next due date',now));
 return executeCommand(db,a,key,op,input,recordId,nextVersion,{id:recordId,version:nextVersion,status:current?.status??'draft'},steps);
}

export async function changePreventivePlan(a:WorkContext,id:string,raw:unknown,key:string,providedWrite?:SqlDatabase):Promise<Record<string,unknown>>{
 access(a,true);const input=preventiveActionSchema.parse(raw),db=providedWrite??(await holdingResources(a.holdingId)).write;
 if(!providedWrite&&input.action==='generate'){
  if(!isTransactionalSqlDatabase(db))throw new ApiError(503,'MAINTENANCE_TRANSACTION_REQUIRED','The transaction runtime is unavailable. Retry when the service is ready.');
  return db.transaction(tx=>changePreventivePlan(a,id,input,key,tx));
 }
 const op=`maintenance.plan.${input.action}`,prior=await replayCommand(db,a,key,op,id,input);if(prior)return prior;
 if(input.action==='generate')await db.prepare('SELECT id FROM maintenance_preventive_plans WHERE id=? AND company_id=? FOR UPDATE').bind(id,a.companyId).run();
 const plan=await loadPlan(db,a,id),now=Date.now(),nextVersion=input.expectedVersion+1;
 if(plan.version!==input.expectedVersion)throw new ApiError(409,'CONFLICT','The preventive plan changed. Reload it before continuing.');
 const steps:SqlStatement[]=[db.prepare('SELECT id FROM maintenance_preventive_plans WHERE id=? FOR UPDATE').bind(id),assertion(db,key,'plan-version',`EXISTS(SELECT 1 FROM maintenance_preventive_plans p WHERE p.id=? AND p.version=? AND ${scope})`,[id,input.expectedVersion,...scopeArgs(a)])];
 let status=plan.status,result:Record<string,unknown>={id,version:nextVersion};
 if(input.action==='activate'){
  requirePermission(a,'records.review');if(!['draft','paused'].includes(plan.status))throw new ApiError(409,'MAINTENANCE_PLAN_STATE','Review a draft or paused plan.');
  if(plan.edited_by===a.userId)throw new ApiError(409,'INDEPENDENT_PLAN_REVIEW_REQUIRED','A different reviewer must activate the current preventive definition.');
  status='active';steps.push(assertion(db,key,'independent-review',`EXISTS(SELECT 1 FROM maintenance_preventive_plans p WHERE p.id=? AND p.edited_by<>? AND ${eligible('p.owner_id')} AND EXISTS(SELECT 1 FROM operational_assets a WHERE a.id=p.asset_id AND a.lifecycle_status IN ('registered','in_service','maintenance')))`,[id,a.userId]),
   db.prepare('UPDATE maintenance_preventive_plans SET approved_by=?,approved_at=? WHERE id=?').bind(a.userId,now,id));
 }else if(input.action==='generate'){
  if(plan.status!=='active')throw new ApiError(409,'MAINTENANCE_PLAN_INACTIVE','Activate this plan before generating its due work.');
  if(plan.owner_id!==a.userId&&!hasPermission(a,'records.review'))throw new ApiError(403,'MAINTENANCE_PLAN_OWNER_REQUIRED','The assigned owner or a reviewer must generate due work.');
  if(plan.next_due_at>now+plan.lead_days*86400000)throw new ApiError(409,'MAINTENANCE_PLAN_NOT_DUE','This occurrence is outside the approved planning window.');
  const next=nextOccurrence(plan.next_due_local,plan.time_zone,plan.interval_days),asset=await db.prepare('SELECT version FROM operational_assets WHERE id=? AND company_id=? FOR UPDATE').bind(plan.asset_id,a.companyId).first<{version:number}>();
  if(!asset)throw new ApiError(409,'MAINTENANCE_PLAN_SOURCE_CHANGED','Reload the current asset before generating work.');
  const definition={title:plan.title,priority:plan.priority,ownerId:plan.owner_id,intervalDays:plan.interval_days,timeZone:plan.time_zone,leadDays:plan.lead_days,dueLocal:plan.next_due_local,dueAt:plan.next_due_at,nextResolvedLocal:next.resolvedLocal};
  // The existing Maintenance owner command, its notification, and the occurrence
  // receipt all commit together. A stale plan cannot leave an orphan work order.
  const work=await createMaintenanceWork(a,{assetId:plan.asset_id,assetVersion:asset.version,title:plan.title,priority:plan.priority,ownerId:plan.owner_id,scheduledAt:plan.next_due_at,reason:input.reason},`${key}:work`,db);
  steps.push(assertion(db,key,'generation-authority',`EXISTS(SELECT 1 FROM maintenance_preventive_plans p WHERE p.id=? AND p.status='active' AND p.next_due_at=? AND ${eligible('p.owner_id')})`,[id,plan.next_due_at]),
   db.prepare('INSERT INTO maintenance_preventive_occurrences VALUES(?,?,?,?,?,?,?,?,?,?)').bind(`preventive-occurrence-${key}`,id,plan.version,work.id,plan.next_due_local,plan.next_due_at,asset.version,JSON.stringify(definition),a.userId,now),
   db.prepare('UPDATE maintenance_preventive_plans SET next_due_local=?,next_due_at=? WHERE id=?').bind(next.local,next.at,id));
  result={...result,workOrderId:work.id,nextDueAt:next.at,nextDueLocal:next.local};
 }else{
  requirePermission(a,'records.review');if(plan.status==='cancelled'||input.action==='pause'&&plan.status!=='active')throw new ApiError(409,'MAINTENANCE_PLAN_STATE','Reload an active plan before changing its schedule.');
  status=input.action==='pause'?'paused':'cancelled';
 }
 steps.push(db.prepare('UPDATE maintenance_preventive_plans SET status=?,version=version+1,updated_at=? WHERE id=?').bind(status,now,id),event(db,key,id,input.action,a,input.reason,now),notice(db,key,id,'Preventive maintenance plan updated: review the current due work',now));
 return executeCommand(db,a,key,op,input,id,nextVersion,{...result,status},steps);
}

export async function preventivePlanWork(a:WorkContext):Promise<WorkItem[]>{
 if(!hasPermission(a,'records.read')||!hasModuleAccess(a.modules,'maintenance'))return [];
 return (await preventivePlans(a)).map(p=>{const terminal=p.status==='cancelled',review=hasPermission(a,'records.review'),own=p.owner_id===a.userId&&Boolean(Number(p.owner_eligible)),due=p.next_due_at<=Date.now()+p.lead_days*86400000,
  canAct=hasPermission(a,'records.write')&&!terminal&&(p.status==='draft'||p.status==='paused'?review&&p.edited_by!==a.userId:p.status==='active'&&due&&(own||review));
  return {source:{kind:'maintenance_plan',id:p.id,version:p.version,href:`/maintenance/plans/${p.id}`,label:`Preventive maintenance · ${p.title}`},id:p.id,type:'maintenance_plan',title:`${p.asset_code} · ${p.title}`,detail:`Preventive · every ${p.interval_days} days · ${p.time_zone}`,href:`/maintenance/plans/${p.id}`,status:p.status,updatedAt:p.updated_at,dueAt:p.next_due_at,queue:terminal?'completed':canAct?'todo':'waiting',canAct,responsibleQueue:'Preventive maintenance owner',nextAction:p.status==='active'?'Generate due work':p.status==='cancelled'?'No further action':'Review and activate preventive plan',blocker:Number(p.owner_eligible)?null:'Assigned maintenance owner is no longer eligible; pause and reassign the plan',relationships:[p.owner_id===a.userId?'assigned':p.created_by===a.userId?'requested':'team']};});
}
