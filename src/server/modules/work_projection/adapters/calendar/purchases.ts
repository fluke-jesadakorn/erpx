import 'server-only';
import {randomUUID} from 'node:crypto';
import {hasPermission} from '@/lib/identity_access/permissions';
import type {CalendarPurchaseControls} from '@/lib/work_projection/calendar-purchases';
import type {CalendarCommand,CalendarItem} from '../../public/rules/calendar-domain';
import type {WorkContext} from '../../../identity_access/public/authorization';
import {currentAssignmentReadScope} from '../../../identity_access/public/record-source-authority';
import {iamMutationGuards} from '../../../identity_access/public/iam-store';
import {lockQueuedCommandAuthority,requireQueuedAccountAuthority} from '../../../identity_access/public/queued-command-authority';
import {workContextGuards} from '../../../workflow/public/commands';
import {workPackageEnforcementMode,companyPurchaseReadLocks,readPurchasedAccess,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure} from '../../../commercial/public/purchase-command-boundary';
import type {CapabilityPurpose} from '../../../commercial/public/rules/packages';
import {flushFinanceDeferredEffects} from '../../../finance/public/finance-transactions';
import {calendarPeopleProjectionSources} from '../../../people_payroll/public/calendar-people-projection-sources';
import {scheduledFeaturePredicate} from '../../../pms/public/hospitality-rollout';
import {lockScheduledFeatureSources} from '../../../pms/public/scheduled-feature-source-locks';
import {isTransactionalSqlDatabase,type SqlDatabase} from '../../../runtime/public/runtime/types';
import {ApiError} from '../../../runtime/public/http';
import {captureCalendarReminderAuthority} from './reminder-authority';

// Calendar historically has no separate Module ID/readiness rule. Its manual
// Work-core events now use the existing Work landing's Operations readiness.
// Roster/Leave/lifecycle and all other owner projections keep their own gates.
export const manualCalendarReleaseModule='operations';
export const isManualCalendarCommand=(input:CalendarCommand)=>input.action.startsWith('appointment.')||input.action.startsWith('announcement.');
export function calendarCommandPurpose(input:CalendarCommand):CapabilityPurpose{
 if(input.action==='appointment.save'||input.action==='announcement.save'||input.action==='announcement.status'&&input.status==='published'||input.action==='appointment.reminder.set'&&input.minutes!==null)return 'new-work';
 return 'settle-existing';
}
async function current(database:SqlDatabase,actor:WorkContext,key:string,guards=false){
 const scope=currentAssignmentReadScope(actor,'workforce_actor_scope');
 if(!await database.prepare(`SELECT 1 FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) workforce_actor_scope WHERE ${scope.where}`).bind(actor.companyId,actor.branchId,actor.departmentId,...scope.args).first())throw new ApiError(403,'CALENDAR_AUTHORITY_CHANGED','Choose a current Calendar account, Company and role.');
 if(guards)await database.batch([...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key)]);
}
async function ready(database:SqlDatabase,actor:WorkContext){
 if(!await database.prepare(`SELECT ${scheduledFeaturePredicate('calendar_release.company_id',manualCalendarReleaseModule)} allowed FROM (SELECT ?::text company_id) calendar_release`).bind(actor.companyId).first<boolean>('allowed'))throw new ApiError(403,'FEATURE_COMING_SOON','Manual Work Calendar is not released for this Company.');
}
export async function calendarPurchaseControls(database:SqlDatabase,actor:WorkContext):Promise<CalendarPurchaseControls|undefined>{
 const mode=await workPackageEnforcementMode();if(mode==='disabled')return;
 await current(database,actor,`calendar-read:${randomUUID()}`);
 const access=await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:'work.calendar'});
 const released=!!await database.prepare(`SELECT ${scheduledFeaturePredicate('calendar_release.company_id',manualCalendarReleaseModule)} allowed FROM (SELECT ?::text company_id) calendar_release`).bind(actor.companyId).first<boolean>('allowed');
 return {newWork:released&&access.newWork,settleExisting:released&&access.settleExisting,history:released&&access.history,revision:access.revision};
}
export async function requireCalendarCurrentActor(database:SqlDatabase,actor:WorkContext){if(await workPackageEnforcementMode()!=='disabled')await current(database,actor,`calendar-current:${randomUUID()}`);}
export function calendarPurchasedItems(items:CalendarItem[],access:CalendarPurchaseControls|undefined){
 if(!access)return items;if(!access.history)return [];
 return items.map(item=>({...item,permittedActions:item.permittedActions.filter(action=>['appointment.save','announcement.save','announcement.published'].includes(action)?access.newWork:access.settleExisting)}));
}
function kind(input:Pick<CalendarCommand,'action'>){return input.action.startsWith('appointment.')?'appointment':'announcement';}
async function image(database:SqlDatabase,companyId:string,type:string,id:string){
 if(type==='announcement')return database.prepare('SELECT to_jsonb(row)::text image FROM calendar_announcements row WHERE id=? AND company_id=?').bind(id,companyId).first<string>('image');
 return database.prepare(`SELECT jsonb_build_object('record',to_jsonb(row),
 'attendees',(SELECT jsonb_agg(to_jsonb(a) ORDER BY a.user_id) FROM calendar_attendees a WHERE a.appointment_id=row.id AND a.company_id=row.company_id),
 'participants',(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.person_id) FROM calendar_appointment_participants p WHERE p.appointment_id=row.id AND p.company_id=row.company_id),
 'series',(SELECT to_jsonb(s) FROM calendar_series s WHERE s.appointment_id=row.id AND s.company_id=row.company_id),
 'exceptions',(SELECT jsonb_agg(to_jsonb(e) ORDER BY e.occurrence_key) FROM calendar_occurrence_exceptions e WHERE e.appointment_id=row.id AND e.company_id=row.company_id),
 'responses',(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.occurrence_key,r.user_id) FROM calendar_occurrence_responses r WHERE r.appointment_id=row.id AND r.company_id=row.company_id))::text image FROM calendar_appointments row WHERE id=? AND company_id=?`).bind(id,companyId).first<string>('image');
}
async function recordAuthority(database:SqlDatabase,actor:WorkContext,input:Pick<CalendarCommand,'action'>,id:string,existing:boolean){
 const type=kind(input),row=await database.prepare(`SELECT company_id,branch_id,department_id,created_by${type==='appointment'?',organizer_person_id':''} FROM ${type==='appointment'?'calendar_appointments':'calendar_announcements'} WHERE id=? AND company_id=?`).bind(id,actor.companyId).first<{company_id:string;branch_id:string|null;department_id:string|null;created_by:string;organizer_person_id?:string|null}>();
 if(!row){if(existing)throw new ApiError(404,'NOT_FOUND','The original Calendar record is unavailable.');return;}
 const placement=currentAssignmentReadScope(actor,'workforce_actor_scope');
 if(!await database.prepare(`SELECT 1 FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) workforce_actor_scope WHERE ${placement.where}`).bind(row.company_id,row.branch_id,row.department_id,...placement.args).first())throw new ApiError(403,'CALENDAR_SOURCE_CHANGED','The original Calendar workspace is outside the selected role.');
 if(type==='announcement'){
  if(!hasPermission(actor,'calendar.announcements.publish')||!await database.prepare("SELECT iam_role_allows(?,?,'calendar.announcements.publish') allowed").bind(actor.role,actor.companyId).first<boolean>('allowed'))throw new ApiError(403,'PERMISSION_REQUIRED','The current role cannot publish or manage Calendar announcements.');return;
 }
 if(input.action==='appointment.respond'||input.action==='appointment.reminder.set'){
  if(!await database.prepare('SELECT 1 FROM calendar_attendees WHERE appointment_id=? AND company_id=? AND user_id=?').bind(id,actor.companyId,actor.userId).first())throw new ApiError(403,'INVITATION_UNAVAILABLE','The original Calendar invitation is unavailable.');return;
 }
 const own=row.created_by===actor.userId||!!await database.prepare(`SELECT 1 FROM (${calendarPeopleProjectionSources.people}) person WHERE person.id=? AND person.company_id=? AND iam_same_person(person.user_id,?,person.company_id)`).bind(row.organizer_person_id??null,actor.companyId,actor.userId).first();
 if(!own&&(!hasPermission(actor,'calendar.appointments.manage')||!await database.prepare("SELECT iam_role_allows(?,?,'calendar.appointments.manage') allowed").bind(actor.role,actor.companyId).first<boolean>('allowed')))throw new ApiError(403,'PERMISSION_REQUIRED','The current role cannot manage this original Calendar appointment.');
}
export async function calendarManualSourceReadLocks(database:SqlDatabase,companyId:string){
 const locks=companyPurchaseReadLocks(database,companyId);
 await database.batch([locks[0],database.prepare('LOCK TABLE calendar_appointments,calendar_announcements,calendar_appointment_participants,calendar_attendees,calendar_series,calendar_occurrence_exceptions,calendar_occurrence_responses,calendar_reminder_deliveries,calendar_history,calendar_reminder_execution_authorizations IN SHARE ROW EXCLUSIVE MODE')]);
 await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(locks.slice(1));
}
export async function requireCalendarReminderRecordAuthority(database:SqlDatabase,actor:WorkContext,id:string,self:boolean){
 await recordAuthority(database,actor,{action:self?'appointment.reminder.set':'appointment.reschedule'},id,true);
 await ready(database,actor);
}
/** Only the normal manual Calendar transaction enters this boundary. Roster
 * and People preparation retain their existing named original owner units. */
export async function withCalendarPurchasedTransaction<Result>(owner:SqlDatabase,actor:WorkContext,input:CalendarCommand,key:string,run:(database:SqlDatabase)=>Promise<Result>):Promise<Result>{
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'CALENDAR_TRANSACTION_REQUIRED','Calendar requires its complete owned Holding transaction.');
 const mode=await workPackageEnforcementMode();if(mode==='disabled')return owner.transaction(run);
 try{return await owner.transaction(async database=>{
  await calendarManualSourceReadLocks(database,actor.companyId);
  const id='id'in input&&input.id?input.id:`calendar-${key}`,operation=`calendar.${input.action}`,receipt=await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND actor_id=? AND operation=? AND record_id=? AND authority_json::jsonb->>'companyId'=?").bind(key,actor.userId,operation,id,actor.companyId).first(),purpose=receipt?'history' as const:calendarCommandPurpose(input),scope={companyId:actor.companyId,capability:'work.calendar',purpose},guard=`calendar-purchase:${randomUUID()}`,boundary=purchaseCommandBoundary(database,mode,guard,scope)!;
  await current(database,actor,guard,true);await ready(database,actor);await checkPurchasedCapability(database,mode,scope);await recordAuthority(database,actor,input,id,!!('id'in input&&input.id)||!!receipt);await database.batch(boundary.before);
  const result=await run(database);
  if(!result||typeof result!=='object'||!('id'in result)||typeof result.id!=='string'||!('version'in result)||typeof result.version!=='number')throw new ApiError(503,'CALENDAR_RESULT_REQUIRED','The normal Calendar owner acknowledgement is unavailable.');
  const resultId=result.id,ids=[...new Set([id,resultId])];
  // The original future split acknowledges the parent, while its actual child
  // keeps the normal calendar-command key identity and an independent version.
  if(input.action==='appointment.reschedule'&&input.scope==='future'&&input.occurrenceId){const child=`calendar-${key}`;if(!ids.includes(child)&&await database.prepare('SELECT 1 FROM calendar_appointments WHERE id=? AND company_id=?').bind(child,actor.companyId).first())ids.push(child);}
  const completed=await Promise.all(ids.map(recordId=>image(database,actor.companyId,kind(input),recordId)));
  const queued=receipt?undefined:await captureCalendarReminderAuthority(database,actor,input,key,ids);
  if(await database.prepare(`SELECT version FROM ${kind(input)==='appointment'?'calendar_appointments':'calendar_announcements'} WHERE id=? AND company_id=?`).bind(resultId,actor.companyId).first<number>('version')!==result.version&&!receipt)throw new ApiError(409,'CALENDAR_SOURCE_CHANGED','The original Calendar acknowledgement no longer matches its source.');
  await flushFinanceDeferredEffects(database);
  for(let i=0;i<ids.length;i++)if(!completed[i]||await image(database,actor.companyId,kind(input),ids[i])!==completed[i])throw new ApiError(409,'CALENDAR_SOURCE_CHANGED','The completed Calendar source changed during deferred effects.');
  await current(database,actor,guard,true);await ready(database,actor);await recordAuthority(database,actor,input,resultId,true);if(queued)await requireQueuedAccountAuthority(database,queued);const results=await database.batch([...boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(guard)]);boundary.observe?.(results);return result;
 });}catch(error){throw purchaseCommandFailure(error)??error;}
}
