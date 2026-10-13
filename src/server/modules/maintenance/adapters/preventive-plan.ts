import { finalRootOwnerProjectionSources as assetsProjectionSources } from "../../assets/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "../../identity_access/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as people_payrollProjectionSources } from "../../people_payroll/public/final-root-owner-projection-sources";
import 'server-only';
import type {WorkContext} from '../../identity_access/public/authorization';
import type {SqlDatabase,SqlStatement} from '../../runtime/public/runtime/types';
import {isTransactionalSqlDatabase} from '../../runtime/public/runtime/types';
import {holdingResources} from '../../runtime/public/runtime';
import {requirePermission} from '../../identity_access/public/iam-permissions';
import {hasPermission} from "@/lib/identity_access/permissions";
import {hasModuleAccess} from '@/data/module-access';
import {ApiError} from '../../runtime/public/http';
import {assertion,executeCommand,replayCommand} from '../../workflow/public/commands';
import {createMaintenanceWork} from '../public/asset-maintenance';
import {createPreventiveApplication,type PreventiveRepository} from '../application/preventive-plan';
import type {Plan,PreventivePlanRow} from '../domain/preventive-plan';
export {preventivePlanSchema,preventiveActionSchema} from '../domain/preventive-plan';
import {preparePreventivePlanNotification as notice} from "../../collaboration/public/maintenance-notifications";
const scope='p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?)';
const scopeArgs=(a:WorkContext)=>[a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId];
const eligible=(user:string)=>`EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) x JOIN (${people_payrollProjectionSources.hr_people}) h ON h.user_id=x.user_id AND h.company_id=x.company_id JOIN (${people_payrollProjectionSources.hr_engagements}) e ON e.person_id=h.id AND e.company_id=h.company_id
 WHERE x.company_id=p.company_id AND x.user_id=${user} AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
 AND iam_principal_active(x.user_id,x.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(x.role,x.company_id,'records.write')
 AND (x.scope_kind='holding' OR x.scope_kind='branch' AND x.scope_node_id=p.branch_id OR x.scope_kind='department' AND x.scope_node_id=p.department_id)
 AND h.status='active' AND h.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))`;
function access(a:WorkContext,write=false){requirePermission(a,write?'records.write':'records.read');if(!hasModuleAccess(a.modules,'maintenance'))throw new ApiError(403,'MODULE_FORBIDDEN','Your current role cannot access Maintenance.');}
function event(db:SqlDatabase,key:string,id:string,action:string,a:WorkContext,note:string,now:number){return db.prepare('INSERT INTO maintenance_preventive_events SELECT ?,id,version,?,?,?,to_jsonb(p),? FROM maintenance_preventive_plans p WHERE id=?').bind(`preventive-event-${key}`,action,a.userId,note,now,id);}


function bindRepository(db:SqlDatabase,a:WorkContext,canStartTransaction:boolean):PreventiveRepository {
 return {
  ...(canStartTransaction&&isTransactionalSqlDatabase(db)?{transaction:<Result>(action:(repository:PreventiveRepository)=>Promise<Result>)=>db.transaction(tx=>action(bindRepository(tx,a,false)))}:{}),
  replay:c=>replayCommand(db,a,c.key,c.operation,c.id,c.input),
  async plans(){return (await db.prepare(`SELECT p.*,a.code asset_code,a.name asset_name,CASE WHEN ${eligible('p.owner_id')} THEN 1 ELSE 0 END owner_eligible,
 (SELECT legal_name FROM (${people_payrollProjectionSources.hr_people}) hr_people WHERE company_id=p.company_id AND user_id=p.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name
 FROM maintenance_preventive_plans p JOIN (${assetsProjectionSources.operational_assets}) a ON a.id=p.asset_id WHERE ${scope} ORDER BY p.next_due_at,p.id LIMIT 300`).bind(...scopeArgs(a)).all<PreventivePlanRow>()).results;},
  plan:id=>db.prepare(`SELECT p.*,a.code asset_code,a.name asset_name FROM maintenance_preventive_plans p JOIN (${assetsProjectionSources.operational_assets}) a ON a.id=p.asset_id WHERE p.id=? AND ${scope}`).bind(id,...scopeArgs(a)).first<Plan>(),
  async lockPlan(id){await db.prepare('SELECT id FROM maintenance_preventive_plans WHERE id=? AND company_id=? FOR UPDATE').bind(id,a.companyId).run();},
  async assetVersion(id,lock){return lock?db.prepare(`SELECT version FROM (${assetsProjectionSources.operational_assets}) operational_assets WHERE id=? AND company_id=? FOR UPDATE`).bind(id,a.companyId).first<{version:number}>():db.prepare(`SELECT version FROM (${assetsProjectionSources.operational_assets}) operational_assets WHERE id=? AND company_id=?`).bind(id,a.companyId).first<{version:number}>();},
  async people(id){return (await db.prepare(`SELECT DISTINCT person.user_id id,person.legal_name name FROM maintenance_preventive_plans p JOIN (${people_payrollProjectionSources.hr_people}) person ON person.company_id=p.company_id WHERE p.id=? AND person.status='active' AND person.merged_into_id IS NULL AND ${eligible('person.user_id')} ORDER BY person.legal_name`).bind(id).all<{id:string;name:string}>()).results;},
  history:async id=>(await db.prepare('SELECT * FROM maintenance_preventive_events WHERE plan_id=? ORDER BY version').bind(id).all<Record<string,unknown>>()).results,
  occurrences:async id=>(await db.prepare('SELECT o.*,w.status,w.version work_version FROM maintenance_preventive_occurrences o JOIN maintenance_work_orders w ON w.id=o.work_order_id WHERE o.plan_id=? ORDER BY o.due_at DESC').bind(id).all<Record<string,unknown>>()).results,
  async save(command,current,facts){const {input,id:recordId,key,operation:op}=command;const {local,dueAt,now,nextVersion}=facts;
 const steps:SqlStatement[]=[db.prepare(`SELECT id FROM (${assetsProjectionSources.operational_assets}) operational_assets WHERE id=? FOR UPDATE`).bind(input.assetId),
 assertion(db,key,'asset-source',`EXISTS(SELECT 1 FROM (${assetsProjectionSources.operational_assets}) p WHERE p.id=? AND p.version=? AND p.lifecycle_status IN ('registered','in_service','maintenance') AND ${scope})`,[input.assetId,input.assetVersion,...scopeArgs(a)])];
 if(current)steps.push(db.prepare('SELECT id FROM maintenance_preventive_plans WHERE id=? FOR UPDATE').bind(recordId),assertion(db,key,'plan-version',`EXISTS(SELECT 1 FROM maintenance_preventive_plans p WHERE p.id=? AND p.version=? AND p.status IN ('draft','paused') AND ${scope})`,[recordId,input.expectedVersion!,...scopeArgs(a)]),
 assertion(db,key,'retained-occurrences','NOT EXISTS(SELECT 1 FROM maintenance_preventive_occurrences WHERE plan_id=? AND due_local>=?)',[recordId,local]),
 db.prepare('UPDATE maintenance_preventive_plans SET title=?,priority=?,owner_id=?,interval_days=?,time_zone=?,first_due_local=?,next_due_local=?,next_due_at=?,lead_days=?,edited_by=?,approved_by=NULL,approved_at=NULL,version=version+1,updated_at=? WHERE id=?').bind(input.title,input.priority,input.ownerId,input.intervalDays,input.timeZone,local,local,dueAt,input.leadDays,a.userId,now,recordId));
 else steps.push(db.prepare(`INSERT INTO maintenance_preventive_plans(id,company_id,branch_id,department_id,asset_id,title,priority,owner_id,interval_days,time_zone,first_due_local,next_due_local,next_due_at,lead_days,created_by,edited_by,created_at,updated_at)
 SELECT ?,company_id,branch_id,department_id,id,?,?,?,?,?,?,?,?,?,?,?,?,? FROM (${assetsProjectionSources.operational_assets}) operational_assets WHERE id=?`).bind(recordId,input.title,input.priority,input.ownerId,input.intervalDays,input.timeZone,local,local,dueAt,input.leadDays,a.userId,a.userId,now,now,input.assetId));
 steps.push(assertion(db,key,'eligible-owner',`EXISTS(SELECT 1 FROM maintenance_preventive_plans p WHERE p.id=? AND ${eligible('p.owner_id')})`,[recordId]),event(db,key,recordId,current?'revise':'create',a,input.reason,now),notice(db,key,recordId,'Review the preventive maintenance plan and next due date',now));
 return executeCommand(db,a,key,op,input,recordId,nextVersion,{id:recordId,version:nextVersion,status:current?.status??'draft'},steps);

  },
  createWork:(input,key)=>createMaintenanceWork(a,input,key,db),
  async change(command,plan,facts){const {input,id,key,operation:op}=command;const {now,status,result,generation}=facts;const nextVersion=input.expectedVersion+1;
 const steps:SqlStatement[]=[db.prepare('SELECT id FROM maintenance_preventive_plans WHERE id=? FOR UPDATE').bind(id),assertion(db,key,'plan-version',`EXISTS(SELECT 1 FROM maintenance_preventive_plans p WHERE p.id=? AND p.version=? AND ${scope})`,[id,input.expectedVersion,...scopeArgs(a)])];
   if(input.action==='activate'){steps.push(assertion(db,key,'independent-review',`EXISTS(SELECT 1 FROM maintenance_preventive_plans p WHERE p.id=? AND p.edited_by<>? AND ${eligible('p.owner_id')} AND EXISTS(SELECT 1 FROM (${assetsProjectionSources.operational_assets}) a WHERE a.id=p.asset_id AND a.lifecycle_status IN ('registered','in_service','maintenance')))`,[id,a.userId]),
   db.prepare('UPDATE maintenance_preventive_plans SET approved_by=?,approved_at=? WHERE id=?').bind(a.userId,now,id));
   }else if(input.action==='generate'){
    if(!generation)throw new Error('PREVENTIVE_GENERATION_FACTS_REQUIRED');
    const {next,definition}=generation,work={id:generation.workOrderId},asset={version:generation.assetVersion};
  steps.push(assertion(db,key,'generation-authority',`EXISTS(SELECT 1 FROM maintenance_preventive_plans p WHERE p.id=? AND p.status='active' AND p.next_due_at=? AND ${eligible('p.owner_id')})`,[id,plan.next_due_at]),
   db.prepare('INSERT INTO maintenance_preventive_occurrences VALUES(?,?,?,?,?,?,?,?,?,?)').bind(`preventive-occurrence-${key}`,id,plan.version,work.id,plan.next_due_local,plan.next_due_at,asset.version,JSON.stringify(definition),a.userId,now),
   db.prepare('UPDATE maintenance_preventive_plans SET next_due_local=?,next_due_at=? WHERE id=?').bind(next.local,next.at,id));
   }
 steps.push(db.prepare('UPDATE maintenance_preventive_plans SET status=?,version=version+1,updated_at=? WHERE id=?').bind(status,now,id),event(db,key,id,input.action,a,input.reason,now),notice(db,key,id,'Preventive maintenance plan updated: review the current due work',now));
 return executeCommand(db,a,key,op,input,id,nextVersion,{...result,status},steps);

  }
 };
}
function application(a:WorkContext,providedWrite?:SqlDatabase){return createPreventiveApplication({actorId:a.userId,transactionBound:Boolean(providedWrite),authorize:write=>access(a,write),authorizeReview:()=>requirePermission(a,'records.review'),canRead:()=>hasPermission(a,'records.read')&&hasModuleAccess(a.modules,'maintenance'),canWrite:()=>hasPermission(a,'records.write'),canReview:()=>hasPermission(a,'records.review'),clock:{now:()=>Date.now()},error:(status,code,message)=>new ApiError(status,code,message),repository:async()=>bindRepository(providedWrite??(await holdingResources(a.holdingId)).write,a,!providedWrite)});}
export async function preventivePlans(a:WorkContext,providedWrite?:SqlDatabase){return application(a,providedWrite).preventivePlans();}
export async function preventivePlanDetail(a:WorkContext,id:string){return application(a).preventivePlanDetail(id);}
export async function savePreventivePlan(a:WorkContext,id:string|null,raw:unknown,key:string){return application(a).savePreventivePlan(id,raw,key);}
export async function changePreventivePlan(a:WorkContext,id:string,raw:unknown,key:string,providedWrite?:SqlDatabase){return application(a,providedWrite).changePreventivePlan(id,raw,key);}
export async function preventivePlanWork(a:WorkContext){return application(a).preventivePlanWork();}
