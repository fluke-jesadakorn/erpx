import 'server-only';
import { nativeClearCommandAssertions } from '@erpx/collaboration-server/adapters/native-command-assertions';
import {randomUUID} from 'node:crypto';
import type {Row} from '@erpx/work-projection-server/domain/calendar/reminders';
import {calendarManualSourceReadLocks,requireCalendarReminderRecordAuthority} from '@erpx/work-projection-server/adapters/calendar/purchases';
import {calendarReminderConfiguration,requireCalendarReminderBinding,appointmentVersionSql} from '@erpx/work-projection-server/adapters/calendar/reminder-authority';
import {occurrenceExceptionPointSql,reminderAttendeePointSql} from '@erpx/work-projection-server/adapters/read-tail-sql';
import {appsPackageEnforcementMode,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure} from '@erpx/commercial-server/public/purchase-command-boundary';
import {flushFinanceDeferredEffects} from '@erpx/finance-server/public/finance-transactions';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {localDateTimeToInstant} from '@erpx/work-projection-ui/lib/calendar-timeline';
import {nativeCalendarRead} from '@erpx/work-projection-ui/lib/native-calendar-read';
const occurrenceExceptionSql=occurrenceExceptionPointSql('cancelled,start_local');
/** A worker receives a candidate, never a trusted SQL snapshot or inferred
 * recipient role. The complete ledger and private notification stay inside
 * this fresh source-leased Holding transaction until final validation. */
export async function withCalendarReminderDelivery(owner:SqlDatabase,row:Row,occurrence:{key:string;startsAt:number;due:number;now:number},run:(database:SqlDatabase)=>Promise<boolean>){
 const mode=await appsPackageEnforcementMode();if(mode!=='enforced')return run(owner);
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'CALENDAR_REMINDER_UNIT_REQUIRED','Calendar reminders require their complete owned Holding transaction.');
 try{return await owner.transaction(async database=>{
  await calendarManualSourceReadLocks(database,row.company_id);
  const source=await database.prepare(reminderAttendeePointSql('a.version,a.status,a.title,a.starts_at,a.ends_at,a.timezone,t.reminder_minutes,t.response')).bind(row.id,row.company_id,row.user_id).first<{version:number;status:string;title:string;starts_at:number;ends_at:number;timezone:string;reminder_minutes:number|null;response:string}>();
  if(!source||source.status!=='scheduled'||source.reminder_minutes===null||source.response==='declined')return false;
  if(source.version!==row.version||source.title!==row.title||source.starts_at!==row.starts_at||source.ends_at!==row.ends_at||source.timezone!==row.timezone||source.reminder_minutes!==row.reminder_minutes)throw new ApiError(409,'CALENDAR_REMINDER_SOURCE_CHANGED','The candidate reminder configuration changed.');
  const image=await calendarReminderConfiguration(database,row.company_id,row.id,row.user_id),authorization=await nativeCalendarRead.read67({value1:row.id,value2:row.company_id,value3:row.user_id});
  if(!image||!authorization)return false;
  const actor=await requireCalendarReminderBinding(database,authorization,image);await requireCalendarReminderRecordAuthority(database,actor,row.id,authorization.action==='appointment.reminder.set');
  const changed=await database.prepare(occurrenceExceptionSql).bind(row.id,row.company_id,occurrence.key).first<{cancelled:boolean;start_local:string|null}>(),response=(await nativeCalendarRead.read61({value1:row.id,value2:row.company_id,value3:occurrence.key,value4:row.user_id}))?.response??null;
  if(changed?.cancelled||response==='declined')return false;
  const startsAt=changed?.start_local?localDateTimeToInstant(changed.start_local,source.timezone):occurrence.startsAt;
  if(startsAt!==occurrence.startsAt||startsAt-source.reminder_minutes*60000!==occurrence.due||occurrence.due>occurrence.now||startsAt<occurrence.now-3600000)throw new ApiError(409,'CALENDAR_REMINDER_SOURCE_CHANGED','The original reminder occurrence changed.');
  const scope={companyId:row.company_id,capability:'work.calendar',purpose:'settle-existing' as const},key=`calendar-reminder:${randomUUID()}`,boundary=purchaseCommandBoundary(database,mode,key,scope)!;
  await checkPurchasedCapability(database,mode,scope);await database.batch(boundary.before);
  const result=await run(database);await flushFinanceDeferredEffects(database);
  if(image!==await calendarReminderConfiguration(database,row.company_id,row.id,row.user_id)||source.version!==await database.prepare(appointmentVersionSql).bind(row.id,row.company_id).first<number>('version')||JSON.stringify(changed)!==JSON.stringify(await database.prepare(occurrenceExceptionSql).bind(row.id,row.company_id,occurrence.key).first())||response!==((await nativeCalendarRead.read61({value1:row.id,value2:row.company_id,value3:occurrence.key,value4:row.user_id}))?.response??null))throw new ApiError(409,'CALENDAR_REMINDER_SOURCE_CHANGED','The original reminder source changed during delivery.');
  await requireCalendarReminderBinding(database,authorization,image);await requireCalendarReminderRecordAuthority(database,actor,row.id,authorization.action==='appointment.reminder.set');
  const results=await database.batch([...boundary.after,await nativeClearCommandAssertions(database, actor.companyId, key)]);boundary.observe?.(results);return result;
 });}catch(error){const failure=purchaseCommandFailure(error)??error;if(failure instanceof ApiError&&['CALENDAR_REMINDER_SOURCE_CHANGED','CALENDAR_REMINDER_RECIPIENT_CHANGED','QUEUED_AUTHORITY_CHANGED','CALENDAR_SOURCE_CHANGED','CALENDAR_AUTHORITY_CHANGED','PERMISSION_REQUIRED','INVITATION_UNAVAILABLE','FEATURE_COMING_SOON','PACKAGE_REQUIRED','PACKAGE_INACTIVE','PACKAGE_EXPIRED','PACKAGE_DRAINING'].includes(failure.code))return false;throw failure;}
}
