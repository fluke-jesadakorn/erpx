import {rosterNotificationReadScope} from '@erpx/server-people_payroll/adapters/roster-notification-purchases';
import 'server-only';
import {randomUUID} from 'node:crypto';
import type {StaffActor} from '@erpx/server-operations/public/staff-policy';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {currentAssignmentReadScope} from '@erpx/server-identity_access/public/record-source-authority';
import {participantSql} from '@erpx/server-workflow/public/approval-participant-projection';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {financialConstraintError} from '@erpx/server-workflow/public/command-guards';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import {ApiError} from '@erpx/server-runtime/public/http';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {peoplePackageEnforcementMode,checkPurchasedCapability,readPurchasedAccess,purchaseCommandBoundary,purchaseCommandFailure,companyPurchaseReadLocks} from '@erpx/server-commercial/public/purchase-command-boundary';
import type {CapabilityPurpose} from '@erpx/server-commercial/public/rules/packages';
import {purchasedCapabilityPredicate,packageClockSql} from '@erpx/server-commercial/public/purchased-capability-sql';
import {workforceSourceReadLocks} from '@erpx/server-people_payroll/adapters/workforce-purchases';
import type {WorkItem} from '@erpx/feature-work_projection/lib/work-item';
import type {FlowView} from '@erpx/feature-shared/lib/flow';
type Kind='time'|'leave'|'overtime';
const tables={time:'hr_time_entries',leave:'leave_requests',overtime:'overtime_requests'} as const;
type Fragment={where:string;args:(string|number|null)[]};
function source(actor:StaffActor,kind:Kind,id:string,company:string,purchased=true){
 const leave=kind==='leave',time=kind==='time',scope=time?'workforce_engagement':'workforce_native',current=currentAssignmentReadScope(actor,'workforce_actor_scope');
 let policy:Fragment;
 if(time){
  const permissions=(['people.time.manage','people.compensation.read','people.compensation.payroll'] as const).map(permission=>currentAssignmentReadScope(actor,'workforce_actor_scope',permission));
  policy={where:`(workforce_person.user_id=? OR workforce_native.submitted_by=? OR ${permissions.map(permission=>`(${permission.where})`).join(' OR ')})`,args:[actor.userId,actor.userId,...permissions.flatMap(permission=>permission.args)]};
 }else if(leave){
  const evidence=currentAssignmentReadScope(actor,'workforce_actor_scope','leave.evidence.read');
  policy={where:`(workforce_native.created_by=? OR workforce_native.decided_by=? OR ${participantSql('leave','workforce_native','?','?')} OR ${participantSql('leave_cancellation','workforce_native','?','?')} OR (${evidence.where}))`,args:[actor.userId,actor.userId,actor.userId,actor.assignmentId,actor.userId,actor.assignmentId,...evidence.args]};
 }else policy={where:`(workforce_native.created_by=? OR ${participantSql('overtime','workforce_native','?','?')})`,args:[actor.userId,actor.userId,actor.assignmentId]};
 const state=time?'workforce_native.status':'workforce_native.state',purpose=time?`CASE WHEN workforce_native.status='submitted' AND workforce_native.submitted_by<>? AND workforce_person.user_id<>? THEN 'settle-existing' ELSE 'new-work' END`:`CASE WHEN workforce_native.state IN ('draft','rejected') THEN 'new-work' ELSE 'settle-existing' END`,purchase=(purpose:CapabilityPurpose)=>purchased?purchasedCapabilityPredicate('workforce_native.company_id','people.workforce',purpose,'workforce_source_clock.now_ms'):'TRUE';
 const canAct=time?`CASE WHEN workforce_native.status='submitted' AND workforce_native.submitted_by<>? AND workforce_person.user_id<>? THEN ${purchase('settle-existing')} ELSE ${purchase('new-work')} END`:`CASE WHEN workforce_native.state IN ('draft','rejected') THEN ${purchase('new-work')} ELSE ${purchase('settle-existing')} END`;
 return {sql:`SELECT workforce_native.id,workforce_native.company_id,${scope}.branch_id,${scope}.department_id,${state} state,${canAct} can_act,jsonb_build_object('source',to_jsonb(workforce_native),'engagement',to_jsonb(workforce_engagement),'person',jsonb_build_object('id',workforce_person.id,'companyId',workforce_person.company_id,'userId',workforce_person.user_id,'status',workforce_person.status,'mergedIntoId',workforce_person.merged_into_id))::text image
 FROM ${tables[kind]} workforce_native ${leave?'LEFT JOIN employees legacy ON legacy.id=workforce_native.employee_id AND legacy.company_id=workforce_native.company_id':''}
 LEFT JOIN hr_engagements workforce_engagement ON workforce_engagement.id=workforce_native.engagement_id AND workforce_engagement.company_id=workforce_native.company_id
 LEFT JOIN hr_people workforce_person ON workforce_person.id=${leave?'COALESCE(workforce_native.person_id,workforce_engagement.person_id,legacy.person_id)':'workforce_engagement.person_id'} AND workforce_person.company_id=workforce_native.company_id
 CROSS JOIN workforce_source_clock CROSS JOIN LATERAL(SELECT workforce_native.company_id,${scope}.branch_id,${scope}.department_id)workforce_actor_scope
 WHERE workforce_native.id=${id} AND workforce_native.company_id=${company} AND ${leave?"(workforce_native.engagement_id IS NULL OR workforce_engagement.id IS NOT NULL) AND (workforce_person.id IS NOT NULL OR legacy.id IS NOT NULL)":"workforce_engagement.id IS NOT NULL AND workforce_person.id IS NOT NULL"}
 AND (?::text IS NULL OR ${scope}.branch_id=?) AND (?::text IS NULL OR ${scope}.department_id=?) AND (${current.where}) AND (${policy.where}) AND ${purchase('history')}`,
 args:[...(time?[actor.userId,actor.userId]:[]),actor.branchId,actor.branchId,actor.departmentId,actor.departmentId,...current.args,...policy.args],purpose};
}
const kinds=Object.keys(tables) as Kind[];
function queries(actor:StaffActor,selected:Kind[],id:string,company:string,purchased=true){const values=selected.map(kind=>source(actor,kind,id,company,purchased));return {sql:`WITH workforce_source_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms) ${values.map(value=>value.sql).join(' UNION ALL ')}`,args:values.flatMap(value=>value.args)};}
function actual(id:string){return kinds.map(kind=>`EXISTS(SELECT 1 FROM ${tables[kind]} actual_workforce WHERE actual_workforce.id=${id})`).join(' OR ');}
/** Exact persisted owner IDs only; unrelated People, Projects worker and
 * Learning learner foundations retain their original contracts. */
export async function workforceSourceReadScope(database:SqlDatabase,actor:WorkContext,surface:'records'|'staff'|'inbox'){
 const mode=await peoplePackageEnforcementMode();if(mode!=='enforced'){if(mode==='shadow')await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:'people.workforce'});return {where:'TRUE',args:[] as unknown[]};}
 const id=surface==='records'?'workspace_records.id':surface==='staff'?'staff_notifications.record_id':'e.source_id',company=surface==='records'?'workspace_records.company_id':surface==='staff'?'staff_notifications.company_id':'e.company_id';
 if(surface==='records'){const visible=queries(actor,kinds,id,company);return {where:`(NOT(module_id='people_payroll' AND (${actual(id)})) OR EXISTS(${visible.sql}))`,args:visible.args};}
 const roster=rosterNotificationReadScope(actor,surface);
 const type=surface==='staff'?'staff_notifications.record_type':'e.source_type',leave=queries(actor,['leave'],id,company),overtime=queries(actor,['overtime'],id,company),time=queries(actor,['time'],id,company);
 return {where:`(${type} NOT IN ('leave','leave_cancellation','overtime','time_entry') OR (${type} IN ('leave','leave_cancellation') AND EXISTS(${leave.sql})) OR (${type}='overtime' AND EXISTS(${overtime.sql})) OR (${type}='time_entry' AND EXISTS(${time.sql}))) AND ${roster.where}`,args:[...leave.args,...overtime.args,...time.args,...roster.args]};
}
async function read(database:SqlDatabase,actor:StaffActor,kind:Kind,id:string,purchased=true){const visible=queries(actor,[kind],'workforce_input.id','workforce_input.company_id',purchased);return database.prepare(`SELECT visible.* FROM(SELECT ?::text id,?::text company_id)workforce_input CROSS JOIN LATERAL(${visible.sql}) visible`).bind(id,actor.companyId,...visible.args).first<{id:string;company_id:string;branch_id:string|null;department_id:string|null;state:string;can_act:boolean;image:string}>();}
export async function requireWorkforceWorkspaceRecordRead(database:SqlDatabase,actor:WorkContext,moduleId:string,id:string){
 const mode=await peoplePackageEnforcementMode();if(mode==='disabled'||moduleId!=='people_payroll')return;
 const actualKind=await database.prepare(`SELECT kind FROM(${kinds.map(kind=>`SELECT '${kind}' kind FROM ${tables[kind]} WHERE id=?`).join(' UNION ALL ')})actual LIMIT 1`).bind(...kinds.map(()=>id)).first<Kind>('kind');if(!actualKind)return;
 if(!await read(database,actor,actualKind,id,false))throw new ApiError(404,'WORKFORCE_SOURCE_NOT_FOUND','The actual Workforce source is unavailable in the current role.');
 await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:'people.workforce',purpose:'history'});
}
/** Repeated after generic Flow enrichment so stale flags and private sources
 * cannot be resurrected by a later shared presentation pass. */
export async function filterWorkforceWork(database:SqlDatabase,actor:WorkContext,items:WorkItem[]){
 const mode=await peoplePackageEnforcementMode();if(mode==='disabled')return items;if(mode==='shadow'){await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:'people.workforce'});return items;}
 const output:WorkItem[]=[];for(const item of items){const kind=item.type==='leave'?'leave':item.type==='overtime'?'overtime':null;if(!kind){output.push(item);continue;}const source=await read(database,actor,kind,item.id);if(!source)continue;
  try{const current=kind==='leave'?await(await import('@erpx/server-people_payroll/public/leave')).getLeave(actor.holdingId,item.id,actor,database):await(await import('@erpx/server-people_payroll/public/overtime')).getOvertime(actor,item.id,database),actions=current.permittedActions.filter(action=>!['cancel','withdraw'].includes(action)),access=current.purchase??await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:'people.workforce'}),allowed=actions.some(action=>['save','submit'].includes(action)?access.newWork:access.settleExisting),canAct=item.canAct&&allowed;
   output.push({...item,canAct,...(!canAct?{queue:item.queue==='completed'?'completed':'waiting',actionKind:'work',relationships:(item.relationships??[]).filter(value=>!['assigned','approval'].includes(value)),flow:item.flow?{...item.flow,canAct:false,canManage:false}:undefined}:{})});
  }catch(error){if(error instanceof ApiError&&[403,404].includes(error.status))continue;throw error;}
 }return output;
}
export async function requireWorkforceFlowHistory(database:SqlDatabase,actor:StaffActor,id:string){
 const mode=await peoplePackageEnforcementMode();if(mode==='disabled')return;
 if(!await read(database,actor,'leave',id,false))throw new ApiError(404,'WORKFORCE_SOURCE_NOT_FOUND','The original Leave source is unavailable in the current role.');
 await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:'people.workforce',purpose:'history'});
}
export async function workforceDocumentFlow(database:SqlDatabase,actor:StaffActor,view:FlowView){if(await peoplePackageEnforcementMode()!=='enforced')return view;const source=await read(database,actor,'leave',view.recordId);if(!source)throw new ApiError(404,'WORKFORCE_SOURCE_NOT_FOUND','The original Leave source is unavailable in the current role.');return source.can_act?view:{...view,canAct:false,canManage:false};}
/** Only the original direct Flow-assignment pool owns this transaction. The
 * actual retained Leave source and current authority repeat after all effects. */
export async function withWorkforceFlowAssignment<Result>(owner:SqlDatabase,actor:StaffActor,id:string,command:{key:string;stepKey:string;saveDefault:boolean},run:(database:SqlDatabase)=>Promise<Result>){
 const mode=await peoplePackageEnforcementMode();if(mode==='disabled')return run(owner);if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'WORKFORCE_TRANSACTION_REQUIRED','Leave routing requires its original Holding transaction.');
 try{return await owner.transaction(async database=>{
  const locks=companyPurchaseReadLocks(database,actor.companyId);await database.batch([locks[0],...workforceSourceReadLocks(database),database.prepare('LOCK TABLE approval_instances,approval_steps,approval_decisions IN SHARE ROW EXCLUSIVE MODE'),...locks.slice(1)]);
  const before=await read(database,actor,'leave',id,false);if(!before)throw new ApiError(404,'WORKFORCE_SOURCE_NOT_FOUND','The original Leave source is unavailable in the current role.');
  const replay=await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation='flow.assign' AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(command.key,id,actor.userId,actor.companyId).first(),purpose=replay?'history' as const:command.saveDefault||['draft','rejected'].includes(before.state)||command.stepKey==='draft'?'new-work' as const:'settle-existing' as const,scope={companyId:actor.companyId,capability:'people.workforce',purpose},key=`workforce-flow:${randomUUID()}`,boundary=purchaseCommandBoundary(database,mode,key,scope)!;
  await checkPurchasedCapability(database,mode,scope);await database.batch(boundary.before);const result=await run(database);await flushFinanceDeferredEffects(database);
  const after=await read(database,actor,'leave',id,false);if(!after||after.image!==before.image)throw new ApiError(409,'WORKFORCE_SOURCE_CHANGED','The actual Leave source or current role changed before routing committed.');
  const final=queries(actor,['leave'],'workforce_input.id','workforce_input.company_id',false);const observations=await database.batch([...boundary.after,assertion(database,key,'workforce-flow-source',`EXISTS(SELECT 1 FROM(SELECT ?::text id,?::text company_id)workforce_input CROSS JOIN LATERAL(${final.sql})visible WHERE visible.image=?)`,[id,actor.companyId,...final.args,before.image]),database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key)]);boundary.observe?.(observations);return result;
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}

/** Exact native record policy for a Server-side product role candidate. This
 * does not depend on the rollout switch and never supplies a WorkContext. */
export async function hasWorkforceProductRecordHistory(database:SqlDatabase,actor:StaffActor,kind:'leave'|'overtime',id:string){return !!await read(database,actor,kind,id,true);}
