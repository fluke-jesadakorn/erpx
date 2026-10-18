import 'server-only';
import {randomUUID} from 'node:crypto';
import type {Row} from '../../domain/calendar/reminders';
import {calendarManualSourceReadLocks,requireCalendarReminderRecordAuthority} from './purchases';
import {calendarReminderConfiguration,requireCalendarReminderBinding,type CalendarReminderAuthorization} from './reminder-authority';
import {workPackageEnforcementMode,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure} from '../../../commercial/public/purchase-command-boundary';
import {flushFinanceDeferredEffects} from '../../../finance/public/finance-transactions';
import {isTransactionalSqlDatabase,type SqlDatabase} from '../../../runtime/public/runtime/types';
import {ApiError} from '../../../runtime/public/http';
import {localDateTimeToInstant} from '@/lib/work_projection/calendar-timeline';
/** A worker receives a candidate, never a trusted SQL snapshot or inferred
 * recipient role. The complete ledger and private notification stay inside
 * this fresh source-leased Holding transaction until final validation. */
export async function withCalendarReminderDelivery(owner:SqlDatabase,row:Row,occurrence:{key:string;startsAt:number;due:number;now:number},run:(database:SqlDatabase)=>Promise<boolean>){
 const mode=await workPackageEnforcementMode();if(mode!=='enforced')return run(owner);
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'CALENDAR_REMINDER_UNIT_REQUIRED','Calendar reminders require their complete owned Holding transaction.');
 try{return await owner.transaction(async database=>{
  await calendarManualSourceReadLocks(database,row.company_id);
  const source=await database.prepare('SELECT a.version,a.status,a.title,a.starts_at,a.ends_at,a.timezone,t.reminder_minutes,t.response FROM calendar_appointments a JOIN calendar_attendees t ON t.appointment_id=a.id AND t.company_id=a.company_id WHERE a.id=? AND a.company_id=? AND t.user_id=?').bind(row.id,row.company_id,row.user_id).first<{version:number;status:string;title:string;starts_at:number;ends_at:number;timezone:string;reminder_minutes:number|null;response:string}>();
  if(!source||source.status!=='scheduled'||source.reminder_minutes===null||source.response==='declined')return false;
  if(source.version!==row.version||source.title!==row.title||source.starts_at!==row.starts_at||source.ends_at!==row.ends_at||source.timezone!==row.timezone||source.reminder_minutes!==row.reminder_minutes)throw new ApiError(409,'CALENDAR_REMINDER_SOURCE_CHANGED','The candidate reminder configuration changed.');
  const image=await calendarReminderConfiguration(database,row.company_id,row.id,row.user_id),authorization=await database.prepare('SELECT * FROM calendar_reminder_execution_authorizations WHERE appointment_id=? AND company_id=? AND recipient_id=? ORDER BY source_version DESC,created_at DESC,command_id DESC LIMIT 1').bind(row.id,row.company_id,row.user_id).first<CalendarReminderAuthorization>();
  if(!image||!authorization)return false;
  const actor=await requireCalendarReminderBinding(database,authorization,image);await requireCalendarReminderRecordAuthority(database,actor,row.id,authorization.action==='appointment.reminder.set');
  const changed=await database.prepare('SELECT cancelled,start_local FROM calendar_occurrence_exceptions WHERE appointment_id=? AND company_id=? AND occurrence_key=?').bind(row.id,row.company_id,occurrence.key).first<{cancelled:boolean;start_local:string|null}>(),response=await database.prepare('SELECT response FROM calendar_occurrence_responses WHERE appointment_id=? AND company_id=? AND occurrence_key=? AND user_id=?').bind(row.id,row.company_id,occurrence.key,row.user_id).first<string>('response');
  if(changed?.cancelled||response==='declined')return false;
  const startsAt=changed?.start_local?localDateTimeToInstant(changed.start_local,source.timezone):occurrence.startsAt;
  if(startsAt!==occurrence.startsAt||startsAt-source.reminder_minutes*60000!==occurrence.due||occurrence.due>occurrence.now||startsAt<occurrence.now-3600000)throw new ApiError(409,'CALENDAR_REMINDER_SOURCE_CHANGED','The original reminder occurrence changed.');
  const scope={companyId:row.company_id,capability:'work.calendar',purpose:'settle-existing' as const},key=`calendar-reminder:${randomUUID()}`,boundary=purchaseCommandBoundary(database,mode,key,scope)!;
  await checkPurchasedCapability(database,mode,scope);await database.batch(boundary.before);
  const result=await run(database);await flushFinanceDeferredEffects(database);
  if(image!==await calendarReminderConfiguration(database,row.company_id,row.id,row.user_id)||source.version!==await database.prepare('SELECT version FROM calendar_appointments WHERE id=? AND company_id=?').bind(row.id,row.company_id).first<number>('version')||JSON.stringify(changed)!==JSON.stringify(await database.prepare('SELECT cancelled,start_local FROM calendar_occurrence_exceptions WHERE appointment_id=? AND company_id=? AND occurrence_key=?').bind(row.id,row.company_id,occurrence.key).first())||response!==await database.prepare('SELECT response FROM calendar_occurrence_responses WHERE appointment_id=? AND company_id=? AND occurrence_key=? AND user_id=?').bind(row.id,row.company_id,occurrence.key,row.user_id).first<string>('response'))throw new ApiError(409,'CALENDAR_REMINDER_SOURCE_CHANGED','The original reminder source changed during delivery.');
  await requireCalendarReminderBinding(database,authorization,image);await requireCalendarReminderRecordAuthority(database,actor,row.id,authorization.action==='appointment.reminder.set');
  const results=await database.batch([...boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key)]);boundary.observe?.(results);return result;
 });}catch(error){const failure=purchaseCommandFailure(error)??error;if(failure instanceof ApiError&&['CALENDAR_REMINDER_SOURCE_CHANGED','CALENDAR_REMINDER_RECIPIENT_CHANGED','QUEUED_AUTHORITY_CHANGED','CALENDAR_SOURCE_CHANGED','CALENDAR_AUTHORITY_CHANGED','PERMISSION_REQUIRED','INVITATION_UNAVAILABLE','FEATURE_COMING_SOON','PACKAGE_REQUIRED','PACKAGE_INACTIVE','PACKAGE_EXPIRED','PACKAGE_DRAINING'].includes(failure.code))return false;throw failure;}
}
