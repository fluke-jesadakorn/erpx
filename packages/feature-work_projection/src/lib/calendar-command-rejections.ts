const actions = new Set([
 'appointment.save','appointment.reschedule','appointment.status','appointment.respond','appointment.reminder.set',
 'announcement.save','announcement.reschedule','announcement.status','task.plan','task.reschedule','task.complete','shift.reschedule',
]);
/** Only native schema rejects and named domain failures in the complete Calendar
 * transaction qualify. Authority/purchase/replay failures are never editable proof. */
export function calendarFirstRollback(api:string,input:Record<string,unknown>,status:number,code:unknown,message?:unknown){
 if(typeof input.action!=='string'||!actions.has(input.action))return false;
 const path=new URL(api,'https://calendar.invalid').pathname,match=/^(?:\/api\/v1\/holdings\/[^/]+)?\/calendar(?:\/(appointments|announcements|tasks|shifts))?$/.exec(path);
 if(!match)return false;
 const prefix:Record<string,string>={appointments:'appointment.',announcements:'announcement.',tasks:'task.',shifts:'shift.'};
 if(match[1]&&!input.action.startsWith(prefix[match[1]]))return false;
 const action=input.action;
 if(status===422){
  if(code==='VALIDATION_FAILED')return true;
  if(code==='CALENDAR_TIME')return action==='appointment.save'||action==='appointment.reschedule';
  if(code==='CALENDAR_STAFF')return action==='appointment.save'||action==='task.plan'||action==='task.reschedule';
  if(code==='CALENDAR_ASSIGNEE'||code==='CLEARANCE_DUE_DATE'||code==='CLEARANCE_ASSIGNEE')return action==='task.plan'||action==='task.reschedule';
  if(code==='CALENDAR_ATTENDEE_UNAVAILABLE'||code==='TOO_MANY_ATTENDEES'||code==='CALENDAR_RECURRENCE_END')return action==='appointment.save';
  if(code==='CLEARANCE_REASON'||code==='CLEARANCE_EVIDENCE')return action==='task.complete';
  if(code==='INVALID_TRANSITION')return action==='task.complete'&&input.status==='not_required';
  if(code==='ROSTER_TIME'||code==='ROSTER_REASON'||code==='ROSTER_DATE')return action==='shift.reschedule';
 }
 if(status===409){
  if(code==='VERSION_CONFLICT')return !!input.id&&['appointment.save','appointment.reschedule','appointment.status','announcement.save','announcement.reschedule','announcement.status','task.plan','task.reschedule','task.complete'].includes(action);
  if(code==='INVALID_TRANSITION')return ['appointment.save','appointment.reschedule','appointment.status','announcement.reschedule','announcement.status','task.plan','task.reschedule','task.complete'].includes(action);
  if(code==='PRIVILEGED_WORKFLOW')return action==='task.complete';
  if(code==='ROSTER_CHANGED'||code==='ROSTER_STARTED'||code==='ROSTER_CONFLICT'||code==='ROSTER_STAFF')return action==='shift.reschedule';
 }
 // This original input validation shares its legacy 403 code with authority
 // denials; only the exact owner literal/action is a rollback proof.
 if(status===403&&code==='CALENDAR_FORBIDDEN'&&action==='announcement.save'&&message==='Choose an audience inside the selected workspace.')return true;
 return false;
}
